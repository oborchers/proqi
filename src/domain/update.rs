//! Stable release and installation-wide update values.

use std::{fmt, str::FromStr};

use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

use super::Timestamp;

const INSTALLATION_ID_BYTES: usize = 32;
const INSTALLATION_ID_HEX: usize = INSTALLATION_ID_BYTES * 2;

/// A canonical stable semantic version without prerelease or build metadata.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableVersion(Version);

impl StableVersion {
    /// Parse the exact Cargo-style version representation.
    ///
    /// # Errors
    ///
    /// Rejects malformed, noncanonical, prerelease, and build-bearing values.
    pub fn parse(value: &str) -> Result<Self, UpdateValueError> {
        let parsed = Version::parse(value).map_err(|_| UpdateValueError::InvalidVersion)?;
        if !parsed.pre.is_empty() || !parsed.build.is_empty() || parsed.to_string() != value {
            return Err(UpdateValueError::InvalidVersion);
        }
        Ok(Self(parsed))
    }

    /// Parse a canonical release tag of the form `vX.Y.Z`.
    ///
    /// # Errors
    ///
    /// Rejects tags without `v` or any unstable or noncanonical version.
    pub fn parse_tag(value: &str) -> Result<Self, UpdateValueError> {
        value
            .strip_prefix('v')
            .ok_or(UpdateValueError::InvalidVersion)
            .and_then(Self::parse)
    }

    /// Return the canonical release tag.
    #[must_use]
    pub fn tag(&self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for StableVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for StableVersion {
    type Err = UpdateValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Strong install context selected before presenting an update action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationKind {
    /// Installed by `oborchers/tap/proqi` as a Homebrew formula.
    HomebrewFormula,
    /// Installed from a Proqi standalone release archive.
    StandaloneArchive,
    /// Source checkout, development build, or unverified installation.
    SourceOrUnknown,
}

/// Privacy-preserving stable identity for one installation location.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InstallationIdentity([u8; INSTALLATION_ID_BYTES]);

impl InstallationIdentity {
    /// Construct from the complete SHA-256 installation digest.
    #[must_use]
    pub const fn from_digest(digest: [u8; INSTALLATION_ID_BYTES]) -> Self {
        Self(digest)
    }

    /// Return the complete digest without truncation.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; INSTALLATION_ID_BYTES] {
        self.0
    }
}

impl fmt::Display for InstallationIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for InstallationIdentity {
    type Err = UpdateValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != INSTALLATION_ID_HEX
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
        {
            return Err(UpdateValueError::InvalidInstallationIdentity);
        }
        let mut digest = [0_u8; INSTALLATION_ID_BYTES];
        for (index, byte) in digest.iter_mut().enumerate() {
            let start = index * 2;
            let pair = &value.as_bytes()[start..start + 2];
            let pair = std::str::from_utf8(pair)
                .map_err(|_| UpdateValueError::InvalidInstallationIdentity)?;
            *byte = u8::from_str_radix(pair, 16)
                .map_err(|_| UpdateValueError::InvalidInstallationIdentity)?;
        }
        Ok(Self(digest))
    }
}

impl Serialize for InstallationIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for InstallationIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(de::Error::custom)
    }
}

/// Verified installation description used by update decisions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Installation {
    /// Stable local identity shared by versions at this installation location.
    pub identity: InstallationIdentity,
    /// Verified installation mechanism.
    pub kind: InstallationKind,
    /// Canonical executable currently running.
    #[serde(skip)]
    pub executable: std::path::PathBuf,
    /// Verified package-manager link used for post-install replacement.
    #[serde(skip)]
    pub restart_executable: Option<std::path::PathBuf>,
}

/// Minimal durable installation-wide update state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateCacheState {
    /// Latest observed stable release.
    pub latest_stable: Option<StableVersion>,
    /// Monotonic generation advanced before each elected refresh attempt.
    pub refresh_generation: u64,
    /// Last successful GitHub check.
    pub last_checked_at: Option<Timestamp>,
    /// Exact version deferred until the next successful startup refresh.
    pub dismissed_version: Option<StableVersion>,
    /// Exact version suppressed until a later stable version exists.
    pub skipped_version: Option<StableVersion>,
    /// Last version observed at the installation path.
    pub observed_installed_version: Option<StableVersion>,
    /// Whether one or more old processes may still need restart.
    pub restart_needed: bool,
    /// Optional bounded entity tag returned by GitHub.
    pub etag: Option<String>,
}

/// Stable update-value validation failure.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UpdateValueError {
    /// Version or tag is malformed, unstable, or noncanonical.
    #[error("stable version is invalid")]
    InvalidVersion,
    /// Installation identity is not canonical lowercase SHA-256 hex.
    #[error("installation identity is invalid")]
    InvalidInstallationIdentity,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::{InstallationIdentity, StableVersion, UpdateCacheState};

    #[test]
    fn stable_versions_are_canonical_and_ordered() {
        let old = StableVersion::parse("0.9.9").expect("old version");
        let new = StableVersion::parse_tag("v0.10.0").expect("new version");
        assert!(new > old);
        assert_eq!(new.to_string(), "0.10.0");
        assert_eq!(new.tag(), "v0.10.0");
        for invalid in ["v0.1.0", "0.1", "01.2.3", "0.1.0-alpha.1", "0.1.0+build"] {
            assert!(StableVersion::parse(invalid).is_err(), "accepted {invalid}");
        }
        assert!(StableVersion::parse_tag("0.1.0").is_err());
    }

    #[test]
    fn installation_identity_preserves_all_digest_bytes() {
        let digest = std::array::from_fn(|index| u8::try_from(index).expect("byte index"));
        let identity = InstallationIdentity::from_digest(digest);
        let encoded = identity.to_string();
        assert_eq!(encoded.len(), 64);
        assert_eq!(InstallationIdentity::from_str(&encoded), Ok(identity));
        assert_eq!(identity.as_bytes(), digest);
        assert!(InstallationIdentity::from_str(&encoded.to_uppercase()).is_err());
        assert!(InstallationIdentity::from_str(&encoded[..62]).is_err());
    }

    #[test]
    fn legacy_update_cache_defaults_the_refresh_generation() {
        let state: UpdateCacheState = serde_json::from_str("{}").expect("legacy cache");
        assert_eq!(state.refresh_generation, 0);
    }
}
