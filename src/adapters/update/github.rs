//! Bounded stable-release discovery through GitHub's HTTPS API.

use std::time::Duration;

use serde::Deserialize;
use ureq::{Agent, http::header};

use crate::{
    domain::StableVersion,
    ports::update::{ReleaseObservation, ReleaseSource, UpdateError},
};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/oborchers/proqi/releases/latest";
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const MAX_RESPONSE_HEADER_BYTES: usize = 16 * 1024;
const MAX_ETAG_BYTES: usize = 256;

/// Fetches the latest supported stable release from the canonical repository.
#[derive(Clone)]
pub struct GitHubReleaseSource {
    agent: Agent,
}

impl GitHubReleaseSource {
    /// Construct a strict HTTPS client with bounded redirects, headers, body, and time.
    #[must_use]
    pub fn new() -> Self {
        let config = Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(2)
            .max_response_header_size(MAX_RESPONSE_HEADER_BYTES)
            .timeout_global(Some(Duration::from_secs(10)))
            .timeout_connect(Some(Duration::from_secs(3)))
            .timeout_recv_response(Some(Duration::from_secs(5)))
            .timeout_recv_body(Some(Duration::from_secs(5)))
            .user_agent(format!("proqi/{}", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Default for GitHubReleaseSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ReleaseSource for GitHubReleaseSource {
    fn latest_stable(&mut self, etag: Option<&str>) -> Result<ReleaseObservation, UpdateError> {
        let mut request = self
            .agent
            .get(LATEST_RELEASE_URL)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(etag) = etag.filter(|value| valid_etag(value)) {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        let mut response = request
            .call()
            .map_err(|error| map_transport_error(&error))?;
        match response.status().as_u16() {
            304 => Ok(ReleaseObservation::NotModified),
            200 => {
                let etag = response
                    .headers()
                    .get(header::ETAG)
                    .and_then(|value| value.to_str().ok())
                    .filter(|value| valid_etag(value))
                    .map(str::to_owned);
                let body = response
                    .body_mut()
                    .with_config()
                    .limit(MAX_RESPONSE_BYTES)
                    .read_to_vec()
                    .map_err(|error| map_transport_error(&error))?;
                parse_release(&body, etag)
            }
            _ => Err(UpdateError::InvalidResponse),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleasePayload {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn parse_release(body: &[u8], etag: Option<String>) -> Result<ReleaseObservation, UpdateError> {
    if body.len() > usize::try_from(MAX_RESPONSE_BYTES).unwrap_or(usize::MAX) {
        return Err(UpdateError::ResponseTooLarge);
    }
    let payload: ReleasePayload =
        serde_json::from_slice(body).map_err(|_| UpdateError::InvalidResponse)?;
    if payload.draft || payload.prerelease {
        return Err(UpdateError::InvalidResponse);
    }
    let version =
        StableVersion::parse_tag(&payload.tag_name).map_err(|_| UpdateError::InvalidResponse)?;
    Ok(ReleaseObservation::Latest { version, etag })
}

fn valid_etag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ETAG_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        && !value.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
}

fn map_transport_error(error: &ureq::Error) -> UpdateError {
    if matches!(error, ureq::Error::BodyExceedsLimit(_)) {
        UpdateError::ResponseTooLarge
    } else {
        UpdateError::Network
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::StableVersion,
        ports::update::{ReleaseObservation, UpdateError},
    };

    use super::{MAX_RESPONSE_BYTES, parse_release, valid_etag};

    #[test]
    fn stable_payload_is_strict_and_preserves_bounded_etag() {
        let observation = parse_release(
            br#"{"tag_name":"v0.2.0","draft":false,"prerelease":false}"#,
            Some("\"release-2\"".to_owned()),
        )
        .expect("stable release");
        assert_eq!(
            observation,
            ReleaseObservation::Latest {
                version: StableVersion::parse("0.2.0").expect("version"),
                etag: Some("\"release-2\"".to_owned()),
            }
        );
    }

    #[test]
    fn unstable_or_malformed_payloads_are_rejected() {
        for body in [
            br#"{"tag_name":"v0.2.0-beta.1","draft":false,"prerelease":true}"#.as_slice(),
            br#"{"tag_name":"v0.2.0","draft":true,"prerelease":false}"#.as_slice(),
            br#"{"tag_name":"0.2.0","draft":false,"prerelease":false}"#.as_slice(),
            br#"{"tag_name":"v0.2.0","draft":false}"#.as_slice(),
            br#"{"tag_name":"v0.2.0","draft":false,"prerelease":false,"extra":1}"#.as_slice(),
        ] {
            assert_eq!(parse_release(body, None), Err(UpdateError::InvalidResponse));
        }
    }

    #[test]
    fn response_and_etag_limits_are_enforced() {
        let oversized = vec![b'x'; usize::try_from(MAX_RESPONSE_BYTES).expect("limit") + 1];
        assert_eq!(
            parse_release(&oversized, None),
            Err(UpdateError::ResponseTooLarge)
        );
        assert!(valid_etag("\"valid\""));
        assert!(!valid_etag("bad\r\netag"));
        assert!(!valid_etag(&"x".repeat(257)));
    }
}
