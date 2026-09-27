//! Stable release and installation-wide update values.

use std::{fmt, str::FromStr};

use semver::Version;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

use super::{InstanceId, ReleaseHighlightAnnouncement, RequestId, SessionId, Timestamp};

const INSTALLATION_ID_BYTES: usize = 32;
const INSTALLATION_ID_HEX: usize = INSTALLATION_ID_BYTES * 2;
/// Maximum exact process replacements retained for one external convergence attempt.
pub const EXTERNAL_RESTART_MAX_EXPECTATIONS: usize = 32;

/// A canonical stable semantic version without prerelease or build metadata.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableVersion(Version);

/// Typed ordering between the running executable and one cached installation observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstalledVersionRelation {
    /// The running executable is the version recorded by coordinated update state.
    Equal,
    /// The running executable predates the recorded installation observation.
    CurrentOlder,
    /// The running executable is newer than the recorded installation observation.
    CurrentNewer,
}

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

    /// Compare this running version with one cached installation observation.
    #[must_use]
    pub fn relation_to_observed(&self, observed: &Self) -> InstalledVersionRelation {
        use std::cmp::Ordering;

        match self.cmp(observed) {
            Ordering::Equal => InstalledVersionRelation::Equal,
            Ordering::Less => InstalledVersionRelation::CurrentOlder,
            Ordering::Greater => InstalledVersionRelation::CurrentNewer,
        }
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

/// Exact old process image and durable session expected to reappear after external convergence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRestartExpectation {
    session_id: SessionId,
    previous_instance_id: InstanceId,
    previous_pid: u32,
    previous_version: StableVersion,
}

impl ExternalRestartExpectation {
    /// Describe one exact quiesced process that must be replaced in place.
    #[must_use]
    pub const fn new(
        session_id: SessionId,
        previous_instance_id: InstanceId,
        previous_pid: u32,
        previous_version: StableVersion,
    ) -> Self {
        Self {
            session_id,
            previous_instance_id,
            previous_pid,
            previous_version,
        }
    }

    /// Durable session that must be restored.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Old process instance that authorized the replacement.
    #[must_use]
    pub const fn previous_instance_id(&self) -> InstanceId {
        self.previous_instance_id
    }

    /// Operating-system process retained by same-pane replacement.
    #[must_use]
    pub const fn previous_pid(&self) -> u32 {
        self.previous_pid
    }

    /// Version advertised by the quiesced process.
    #[must_use]
    pub const fn previous_version(&self) -> &StableVersion {
        &self.previous_version
    }
}

/// Bounded durable authority for one unfinished external replacement cohort.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExternalRestartPending {
    target_version: StableVersion,
    operation_id: RequestId,
    expectations: Vec<ExternalRestartExpectation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    acknowledged: Vec<ExternalRestartExpectation>,
}

impl ExternalRestartPending {
    /// Validate one exact external restart cohort.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, duplicate, zero-PID, or non-older expectations.
    pub fn new(
        target_version: StableVersion,
        operation_id: RequestId,
        expectations: Vec<ExternalRestartExpectation>,
    ) -> Result<Self, UpdateValueError> {
        Self::from_parts(target_version, operation_id, expectations, Vec::new())
    }

    /// Exact installed version every replacement must run.
    #[must_use]
    pub const fn target_version(&self) -> &StableVersion {
        &self.target_version
    }

    /// Shared convergence operation authorizing every replacement.
    #[must_use]
    pub const fn operation_id(&self) -> RequestId {
        self.operation_id
    }

    /// Exact bounded replacement cohort.
    #[must_use]
    pub fn expectations(&self) -> &[ExternalRestartExpectation] {
        &self.expectations
    }

    /// Exact unfinished expectation for one durable session.
    #[must_use]
    pub fn expectation_for_session(
        &self,
        session_id: SessionId,
    ) -> Option<&ExternalRestartExpectation> {
        self.expectations
            .iter()
            .find(|expectation| expectation.session_id == session_id)
    }

    /// Whether one exact session was durably acknowledged by manual recovery.
    #[must_use]
    pub fn manually_acknowledged(&self, session_id: SessionId) -> bool {
        self.acknowledged
            .iter()
            .any(|expectation| expectation.session_id == session_id)
    }

    /// Remove one manually restored exact session from the unfinished cohort.
    ///
    /// Returns `None` when that restoration completed the cohort.
    ///
    /// # Errors
    ///
    /// Rejects a session that is not part of this exact pending cohort.
    pub fn after_manual_resume(
        &self,
        session_id: SessionId,
    ) -> Result<Option<Self>, UpdateValueError> {
        if self.expectation_for_session(session_id).is_none() {
            return Err(UpdateValueError::InvalidExternalRestart);
        }
        let mut remaining = self.expectations.clone();
        let index = remaining
            .iter()
            .position(|expectation| expectation.session_id == session_id)
            .ok_or(UpdateValueError::InvalidExternalRestart)?;
        let restored = remaining.remove(index);
        if remaining.is_empty() {
            Ok(None)
        } else {
            let mut acknowledged = self.acknowledged.clone();
            acknowledged.push(restored);
            Self::from_parts(
                self.target_version.clone(),
                self.operation_id,
                remaining,
                acknowledged,
            )
            .map(Some)
        }
    }

    fn from_parts(
        target_version: StableVersion,
        operation_id: RequestId,
        expectations: Vec<ExternalRestartExpectation>,
        acknowledged: Vec<ExternalRestartExpectation>,
    ) -> Result<Self, UpdateValueError> {
        let all = expectations.iter().chain(&acknowledged).collect::<Vec<_>>();
        let valid_len = !expectations.is_empty() && all.len() <= EXTERNAL_RESTART_MAX_EXPECTATIONS;
        let valid_entries = all.iter().all(|expectation| {
            expectation.previous_pid > 0 && expectation.previous_version < target_version
        });
        let unique = all.iter().enumerate().all(|(index, expectation)| {
            all[index + 1..].iter().all(|other| {
                expectation.session_id != other.session_id
                    && expectation.previous_instance_id != other.previous_instance_id
            })
        });
        if !valid_len || !valid_entries || !unique {
            return Err(UpdateValueError::InvalidExternalRestart);
        }
        Ok(Self {
            target_version,
            operation_id,
            expectations,
            acknowledged,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalRestartPendingWire {
    target_version: StableVersion,
    operation_id: RequestId,
    expectations: Vec<ExternalRestartExpectation>,
    #[serde(default)]
    acknowledged: Vec<ExternalRestartExpectation>,
}

impl<'de> Deserialize<'de> for ExternalRestartPending {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ExternalRestartPendingWire::deserialize(deserializer)?;
        Self::from_parts(
            wire.target_version,
            wire.operation_id,
            wire.expectations,
            wire.acknowledged,
        )
        .map_err(de::Error::custom)
    }
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
    /// Exact unfinished replacement cohort for a verified external upgrade.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_restart: Option<ExternalRestartPending>,
    /// Exact initiating-session announcement from one verified in-app upgrade.
    pub release_highlights: Option<ReleaseHighlightAnnouncement>,
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
    /// External replacement authority is empty, oversized, duplicated, or inconsistent.
    #[error("external restart authority is invalid")]
    InvalidExternalRestart,
}

#[cfg(test)]
mod tests;
