//! Installation-aware stable release update boundaries.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    Installation, InstallationIdentity, InstallationKind, InstanceId, ReleaseHighlightAnnouncement,
    RequestId, SessionId, StableVersion, Timestamp, UpdateCacheState,
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

    /// Atomically begin a refresh when the caller still represents the observed generation.
    ///
    /// Passing `None` forces an explicitly requested refresh. Passing a generation coalesces
    /// concurrent startup checks that observed the same state. The returned state contains the
    /// incremented generation, while `None` means another startup already advanced it.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure or generation overflow.
    fn begin_refresh(
        &self,
        installation: InstallationIdentity,
        observed_generation: Option<u64>,
    ) -> Result<Option<UpdateCacheState>, UpdateError>;

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

    /// Defer one exact release until the next successful startup refresh.
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

    /// Durably target one verified in-app upgrade announcement.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn record_release_highlights(
        &self,
        installation: InstallationIdentity,
        announcement: ReleaseHighlightAnnouncement,
    ) -> Result<UpdateCacheState, UpdateError>;

    /// Durably acknowledge one exact matching announcement.
    ///
    /// Returns false when no unacknowledged matching record remains.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn acknowledge_release_highlights(
        &self,
        installation: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError>;
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

/// One in-memory peer replacement the coordinator must observe before announcing an upgrade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateReplacementExpectation {
    /// Session that must be restored under the target executable.
    pub session_id: SessionId,
    /// Old process identity that must be replaced rather than rediscovered.
    pub previous_instance_id: InstanceId,
}

/// Read-only cancellation observed by bounded update coordination waits.
pub trait UpdateCancellation: Send + Sync {
    /// Whether the owning update lane is shutting down.
    fn is_cancelled(&self) -> bool;
}

impl UpdateCancellation for () {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Verified current-user live-instance source for update coordination.
pub trait UpdateInstanceRegistry {
    /// Return one complete verified scan.
    ///
    /// # Errors
    ///
    /// Returns a typed registry or process-verification failure.
    fn active_instances(&self) -> Result<Vec<InstanceInfo>, UpdateError>;

    /// Wait a bounded interval for every peer session to reappear under the exact target.
    ///
    /// Returns the previous instance identities that did not converge. Expectations are
    /// ephemeral coordinator memory and are never persisted.
    ///
    /// # Errors
    ///
    /// Returns a typed registry or process-verification failure.
    fn wait_for_replacements(
        &self,
        installation: InstallationIdentity,
        target: &StableVersion,
        expected: &[UpdateReplacementExpectation],
        timeout: Duration,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<Vec<InstanceId>, UpdateError>;
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
