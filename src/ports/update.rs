//! Installation-aware stable release update boundaries.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    Installation, InstallationIdentity, InstallationKind, InstanceId, RequestId, SessionId,
    StableVersion, Timestamp, UpdateCacheState,
};

use super::runtime::InstanceInfo;

/// Current ephemeral all-session update protocol.
pub const UPDATE_CONTROL_PROTOCOL_VERSION: u32 = 1;

/// One bounded response from the canonical stable-release source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseObservation {
    /// GitHub returned a current stable release.
    Latest {
        /// Canonical stable version.
        version: StableVersion,
        /// Optional bounded HTTP entity tag.
        etag: Option<String>,
    },
    /// Cached release metadata remains current.
    NotModified,
}

/// Installation-wide lock purpose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateLockKind {
    /// Own the one permitted background network refresh.
    Refresh,
    /// Own the one actionable update prompt.
    Prompt,
    /// Own the one approved installer invocation.
    Installer,
}

/// RAII installation lock released on drop or process exit.
pub trait UpdateLease: Send {}

/// Stable-release source such as the GitHub Releases API.
pub trait ReleaseSource {
    /// Fetch the latest stable release with optional safe cache metadata.
    ///
    /// # Errors
    ///
    /// Returns bounded network, protocol, or response validation failures.
    fn latest_stable(
        &mut self,
        installation: InstallationKind,
        etag: Option<&str>,
    ) -> Result<ReleaseObservation, UpdateError>;
}

/// Deterministic installation-context detection.
pub trait InstallDetector {
    /// Identify the running executable and verified installation mechanism.
    ///
    /// # Errors
    ///
    /// Returns a typed path or package-metadata failure.
    fn detect(&self) -> Result<Installation, UpdateError>;
}

/// Private, atomic, installation-wide update state and election locks.
pub trait UpdateStateStore {
    /// Read current state. Corrupt state is represented as an empty cache.
    ///
    /// # Errors
    ///
    /// Returns only filesystem-safety or permission failures.
    fn load(&self, installation: InstallationIdentity) -> Result<UpdateCacheState, UpdateError>;

    /// Try to own one installation-wide operation without waiting.
    ///
    /// # Errors
    ///
    /// Returns a filesystem or lock failure.
    fn try_lock(
        &self,
        installation: InstallationIdentity,
        kind: UpdateLockKind,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError>;

    /// Atomically merge a successful release observation.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn record_success(
        &self,
        installation: InstallationIdentity,
        observed: ReleaseObservation,
        installed: StableVersion,
        checked_at: Timestamp,
    ) -> Result<UpdateCacheState, UpdateError>;

    /// Defer one exact release until a later stale refresh.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn dismiss(
        &self,
        installation: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError>;

    /// Suppress one exact release until a newer release exists.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn skip(
        &self,
        installation: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError>;

    /// Record verified installation progress required for later convergence.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn record_restart_state(
        &self,
        installation: InstallationIdentity,
        installed: StableVersion,
        restart_needed: bool,
    ) -> Result<UpdateCacheState, UpdateError>;
}

/// Bounded readiness request sent to one verified live participant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdatePrepareRequest {
    /// Shared identity for the complete update attempt.
    pub operation_id: RequestId,
    /// Exact stable release requested by the coordinator.
    pub target_version: StableVersion,
    /// Installation all participants must share.
    pub installation_identity: InstallationIdentity,
    /// Absolute domain deadline after which participants resume normal use.
    pub deadline: Timestamp,
}

/// Participant readiness without user content or arbitrary diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "readiness")]
pub enum UpdatePrepareReply {
    /// Pending edits and durable effects have been flushed.
    Ready {
        /// Verified participant process.
        instance_id: InstanceId,
        /// Session that will be resumed after replacement.
        session_id: SessionId,
    },
    /// Participant could not safely enter the barrier.
    Blocked {
        /// Verified participant process.
        instance_id: InstanceId,
        /// Stable content-free reason code.
        code: String,
    },
}

/// One post-install restart request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateRestartRequest {
    /// Attempt previously acknowledged by this participant.
    pub operation_id: RequestId,
    /// Newly installed stable version.
    pub installed_version: StableVersion,
}

/// Restart acknowledgement before terminal cleanup and Unix `exec`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateRestartReply {
    /// Participant receiving the request.
    pub instance_id: InstanceId,
    /// Whether the participant accepted responsibility for replacement.
    pub accepted: bool,
}

/// Verified current-user live-instance source for update coordination.
pub trait UpdateInstanceRegistry {
    /// Return one complete verified scan.
    ///
    /// # Errors
    ///
    /// Returns a typed registry or process-verification failure.
    fn active_instances(&self) -> Result<Vec<InstanceInfo>, UpdateError>;
}

/// Typed update coordination over the existing owner-control endpoint.
pub trait UpdateParticipantGateway {
    /// Flush and quiesce one verified participant.
    ///
    /// # Errors
    ///
    /// Returns a verified transport, timeout, or participant failure.
    fn prepare(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError>;

    /// Release one participant back to ordinary use after an aborted attempt.
    ///
    /// # Errors
    ///
    /// Returns a verified transport or participant failure.
    fn release(
        &mut self,
        participant: &InstanceInfo,
        operation_id: RequestId,
    ) -> Result<(), UpdateError>;

    /// Ask one post-install participant to clean up and replace itself.
    ///
    /// # Errors
    ///
    /// Returns a verified transport, timeout, or participant failure.
    fn restart(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdateRestartRequest,
    ) -> Result<UpdateRestartReply, UpdateError>;
}

/// Sole typed authority for the exact supported Homebrew upgrade command.
pub trait HomebrewInstaller {
    /// Run one direct formula upgrade without a shell.
    ///
    /// # Errors
    ///
    /// Returns a process, exit-status, or installed-version verification failure.
    fn upgrade(&mut self, expected: &StableVersion) -> Result<StableVersion, UpdateError>;
}

/// Replaces the current Unix process after all terminal-owned resources are released.
pub trait ProcessReplacer {
    /// Replace this process with the verified executable resuming one session.
    ///
    /// # Errors
    ///
    /// Returns only when process replacement is unsupported or `exec` fails.
    fn replace(
        &self,
        executable: &std::path::Path,
        session_id: SessionId,
        state_root: Option<&std::path::Path>,
    ) -> Result<(), UpdateError>;
}

/// Update boundary failure without user content.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UpdateError {
    /// HTTPS transport or TLS failed.
    #[error("stable release check failed")]
    Network,
    /// Source returned a status or JSON shape outside the supported contract.
    #[error("stable release response is invalid")]
    InvalidResponse,
    /// Bounded metadata or response limit was exceeded.
    #[error("stable release response exceeded its limit")]
    ResponseTooLarge,
    /// Installation identity or package metadata could not be verified.
    #[error("installation context could not be verified: {0}")]
    Installation(String),
    /// Private cache path, permission, lock, or atomic write failed.
    #[error("update state failed: {0}")]
    State(String),
    /// Verified participant discovery or local coordination failed.
    #[error("update coordination failed: {0}")]
    Coordination(String),
    /// Exact Homebrew formula upgrade failed or returned an ambiguous status.
    #[error("Homebrew update failed")]
    InstallerFailed,
}
