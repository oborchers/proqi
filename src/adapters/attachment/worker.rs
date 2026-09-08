//! Private JSON protocol for killable attachment filesystem batches.

use std::{
    io::{Read as _, Write as _},
    path::Path,
    process::ExitCode,
};

use serde::{Deserialize, Serialize};

use crate::ports::attachment_accessibility::{
    AttachmentAccessFailure, AttachmentAccessibility, AttachmentAvailability,
};

use super::FileAttachmentAccessibility;

const PROTOCOL_VERSION: u8 = 2;
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_BATCH_PATHS: usize = 32;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkerRequest {
    version: u8,
    paths: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkerResponse {
    version: u8,
    outcomes: Vec<WorkerOutcome>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
enum WorkerOutcome {
    Available,
    InCloud,
    Downloading,
    Inaccessible(String),
}

pub(crate) fn encode_request(paths: Vec<String>) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&WorkerRequest {
        version: PROTOCOL_VERSION,
        paths,
    })
}

fn decode_request(bytes: &[u8]) -> Result<WorkerRequest, ()> {
    let request: WorkerRequest = serde_json::from_slice(bytes).map_err(|_| ())?;
    if request.version != PROTOCOL_VERSION || request.paths.len() > MAX_BATCH_PATHS {
        return Err(());
    }
    Ok(request)
}

pub(crate) fn decode_response(
    bytes: &[u8],
    expected: usize,
) -> Result<Vec<Result<AttachmentAvailability, AttachmentAccessFailure>>, ()> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(());
    }
    let response: WorkerResponse = serde_json::from_slice(bytes).map_err(|_| ())?;
    if response.version != PROTOCOL_VERSION || response.outcomes.len() != expected {
        return Err(());
    }
    response
        .outcomes
        .into_iter()
        .map(|outcome| match outcome {
            WorkerOutcome::Available => Ok(Ok(AttachmentAvailability::Available)),
            WorkerOutcome::InCloud => Ok(Ok(AttachmentAvailability::InCloud)),
            WorkerOutcome::Downloading => Ok(Ok(AttachmentAvailability::Downloading)),
            WorkerOutcome::Inaccessible(code) => {
                AttachmentAccessFailure::from_diagnostic_code(&code)
                    .map(Err)
                    .ok_or(())
            }
        })
        .collect()
}

pub(crate) fn run_stdio() -> ExitCode {
    match execute_stdio() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

fn execute_stdio() -> Result<(), ()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES.saturating_add(1))
        .read_to_end(&mut input)
        .map_err(|_| ())?;
    if u64::try_from(input.len()).map_err(|_| ())? > MAX_REQUEST_BYTES {
        return Err(());
    }
    let request = decode_request(&input)?;
    let mut checker = FileAttachmentAccessibility;
    let outcomes = request
        .paths
        .iter()
        .map(|path| match checker.check(Path::new(path)) {
            Ok(AttachmentAvailability::Available) => WorkerOutcome::Available,
            Ok(AttachmentAvailability::InCloud) => WorkerOutcome::InCloud,
            Ok(AttachmentAvailability::Downloading) => WorkerOutcome::Downloading,
            Err(failure) => WorkerOutcome::Inaccessible(failure.diagnostic_code().to_owned()),
        })
        .collect();
    let output = serde_json::to_vec(&WorkerResponse {
        version: PROTOCOL_VERSION,
        outcomes,
    })
    .map_err(|_| ())?;
    std::io::stdout().write_all(&output).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use crate::ports::attachment_accessibility::{AttachmentAccessFailure, AttachmentAvailability};

    use super::{MAX_RESPONSE_BYTES, decode_request, decode_response, encode_request};

    #[test]
    fn protocol_round_trips_unicode_paths_and_rejects_wrong_counts() {
        let request =
            encode_request(vec!["/tmp/Grüße 第一.txt".to_owned()]).expect("request serialization");
        assert!(
            String::from_utf8(request)
                .expect("UTF-8 JSON")
                .contains("Grüße")
        );
        assert_eq!(
            decode_response(br#"{"version":2,"outcomes":[{"state":"available"}]}"#, 1),
            Ok(vec![Ok(AttachmentAvailability::Available)])
        );
        assert!(decode_response(br#"{"version":2,"outcomes":[]}"#, 1).is_err());
    }

    #[test]
    fn protocol_decodes_every_availability_and_failure_state() {
        let response = br#"{"version":2,"outcomes":[{"state":"available"},{"state":"in_cloud"},{"state":"downloading"},{"state":"inaccessible","reason":"missing"},{"state":"inaccessible","reason":"permission_denied"},{"state":"inaccessible","reason":"unmounted"},{"state":"inaccessible","reason":"unreadable"},{"state":"inaccessible","reason":"io"},{"state":"inaccessible","reason":"timed_out"},{"state":"inaccessible","reason":"cancelled"}]}"#;
        assert_eq!(
            decode_response(response, 10),
            Ok(vec![
                Ok(AttachmentAvailability::Available),
                Ok(AttachmentAvailability::InCloud),
                Ok(AttachmentAvailability::Downloading),
                Err(AttachmentAccessFailure::Missing),
                Err(AttachmentAccessFailure::PermissionDenied),
                Err(AttachmentAccessFailure::Unmounted),
                Err(AttachmentAccessFailure::Unreadable),
                Err(AttachmentAccessFailure::Io),
                Err(AttachmentAccessFailure::TimedOut),
                Err(AttachmentAccessFailure::Cancelled),
            ])
        );
    }

    #[test]
    fn protocol_rejects_unknown_wrong_incomplete_and_oversized_responses() {
        for invalid in [
            br#"{"version":1,"outcomes":[{"state":"available"}]}"#.as_slice(),
            br#"{"version":2,"outcomes":[{"state":"unknown"}]}"#.as_slice(),
            br#"{"version":2,"outcomes":[{"state":"inaccessible","reason":"other"}]}"#.as_slice(),
            br#"{"version":2,"outcomes":[{"state":"available","extra":true}]}"#.as_slice(),
            br#"{"version":2,"outcomes":[{"state":"available"}],"extra":true}"#.as_slice(),
            br#"{"version":2,"outcomes":["#.as_slice(),
        ] {
            assert!(decode_response(invalid, 1).is_err());
        }
        assert!(decode_response(&vec![b' '; MAX_RESPONSE_BYTES + 1], 0).is_err());
    }

    #[test]
    fn request_envelope_rejects_wrong_versions_and_unknown_fields() {
        assert!(decode_request(br#"{"version":1,"paths":[]}"#).is_err());
        assert!(decode_request(br#"{"version":2,"paths":[],"unexpected":true}"#).is_err());
        let oversized_count = serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "paths": vec!["/tmp/item"; 33],
        }))
        .expect("request fixture");
        assert!(decode_request(&oversized_count).is_err());
    }
}
