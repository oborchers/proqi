//! Installation-aware stable release update boundaries.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    ExternalRestartPending, Installation, InstallationIdentity, InstallationKind, InstanceId,
    ReleaseHighlightAnnouncement, RequestId, SessionId, StableVersion, Timestamp, UpdateCacheState,
};

use super::runtime::InstanceInfo;

mod convergence;
mod external;

pub use convergence::{ExternalCacheTransition, UpdateLease, UpdateLockKind};
pub use external::{
    ExternalUpgradeAdoptionAuthority, ExternalUpgradeAuthorityError, ProcessReplacer,
};

/// Current all-session protocol; v3 requires durable external-restart compatibility.
pub const UPDATE_CONTROL_PROTOCOL_VERSION: u32 = 3;

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

    /// Try to enter the schema-startup side of the convergence admission gate.
    ///
    /// Production implementations permit concurrent startup readers and exclude them against
    /// one `Convergence` owner. An interactive process releases the lease after publishing its
    /// owner-control endpoint. Without a published endpoint it retains the lease for its lifetime,
    /// so coordination cannot install behind an owner it cannot prepare. A non-interactive command
    /// holds the lease until completion.
    ///
    /// # Errors
    ///
    /// Returns a filesystem or lock failure.
    fn try_startup_lock(
        &self,
        installation: InstallationIdentity,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        self.try_lock(installation, UpdateLockKind::Convergence)
    }

    /// Wait a bounded interval to enter shared schema-startup admission.
    ///
    /// Production implementations use this for bounded lock handoff and concurrent startup
    /// revalidation while an active convergence owner finishes.
    ///
    /// # Errors
    ///
    /// Returns a filesystem or lock failure.
    fn wait_for_startup_lock(
        &self,
        installation: InstallationIdentity,
        _timeout: Duration,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        self.try_startup_lock(installation)
    }

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

    /// Adopt a verified newer external installation only when the cached observation still
    /// matches the value inspected under convergence ownership.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn reconcile_external_upgrade(
        &self,
        installation: InstallationIdentity,
        expected_observed: &StableVersion,
        installed: &StableVersion,
        pending: Option<&ExternalRestartPending>,
    ) -> Result<ExternalCacheTransition, UpdateError>;

    /// Clear external restart state only after exact replacement readiness was verified.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn complete_external_restart(
        &self,
        installation: InstallationIdentity,
        installed: &StableVersion,
        pending: &ExternalRestartPending,
    ) -> Result<ExternalCacheTransition, UpdateError>;

    /// Acknowledge one exact pending session only after an explicit manual resume owns its lease.
    ///
    /// The transition retains every other unfinished expectation and clears restart state only
    /// when this session was the final member of the exact cohort.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn acknowledge_external_resume(
        &self,
        installation: InstallationIdentity,
        installed: &StableVersion,
        pending: &ExternalRestartPending,
        session_id: SessionId,
    ) -> Result<ExternalCacheTransition, UpdateError>;

    /// Atomically complete one exact pending restart transition.
    ///
    /// Distinguishes the caller that clears the pending restart, an earlier
    /// exact completion whose announcement still awaits dismissal, and a cache
    /// mismatch.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn complete_restart(
        &self,
        installation: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<RestartCompletion, UpdateError>;

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

    /// Atomically discard one exact unacknowledged announcement after restart rejection.
    ///
    /// Returns false when the cached announcement no longer describes this upgrade.
    ///
    /// # Errors
    ///
    /// Returns a private atomic cache-write failure.
    fn discard_release_highlights(
        &self,
        installation: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError>;

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

/// Exact result of checking the board-ready restart boundary under the cache lock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartCompletion {
    /// This caller atomically cleared the pending restart.
    Completed,
    /// The same exact announcement was completed earlier and remains pending dismissal.
    AlreadyComplete,
    /// Cached state belongs to another target or announcement.
    Mismatch,
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

/// Irreversible post-install request to stop schema use before any replacement starts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateQuiesceRequest {
    /// Attempt previously acknowledged during preparation.
    pub operation_id: RequestId,
    /// Exact installed release every participant must enter next.
    pub installed_version: StableVersion,
}

/// Proof that one exact prepared session released its shared schema lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateQuiesceReply {
    /// Exact old process that became irreversibly quiescent.
    pub instance_id: InstanceId,
    /// Stable session that must be resumed by replacement or manually.
    pub session_id: SessionId,
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
    /// Operating-system process retained by same-pane Unix `exec`.
    pub previous_pid: u32,
    /// Update attempt that authorized the exact replacement.
    pub operation_id: RequestId,
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

    /// Wait a bounded interval for every peer session to reappear through its accepted same-process
    /// replacement under the exact target and a live owner-control endpoint.
    ///
    /// Returns the previous instance identities that did not converge. External coordination can
    /// persist the same bounded typed expectations until exact completion is revalidated.
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

    /// Irreversibly stop schema-dependent work and release one shared schema lease.
    ///
    /// # Errors
    ///
    /// Returns a verified transport, timeout, or participant failure. A transport failure is
    /// ambiguous, so the coordinator must never assume that the participant is still writable.
    fn quiesce(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdateQuiesceRequest,
    ) -> Result<UpdateQuiesceReply, UpdateError>;

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

/// Sole typed authority for one verified installation-method-aware upgrade.
pub trait UpdateInstaller {
    /// Install one exact release through the verified owner of this installation.
    ///
    /// # Errors
    ///
    /// Returns a process, exit-status, or installed-version verification failure.
    fn upgrade(&mut self, expected: &StableVersion) -> Result<StableVersion, UpdateError>;
}

/// Fetches and authenticates the exact release-attached standalone installer.
pub trait StandaloneInstallerSource {
    /// Return the installer bytes only after its exact release checksum verifies.
    ///
    /// # Errors
    ///
    /// Returns bounded transport, response, checksum, or size failures.
    fn verified_installer(&mut self, expected: &StableVersion) -> Result<Vec<u8>, UpdateError>;
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
    /// The verified installation owner failed or returned an ambiguous status.
    #[error("update installation failed")]
    InstallerFailed,
}
