//! Persistence facade expressed in domain terms.

mod browser_history;
mod capture;
mod compaction;
mod error;
mod migration;
mod onboarding;
mod operation;
mod receipt;
mod session;
mod session_request;
mod submission_route;

use serde::{Deserialize, Serialize};

use crate::domain::{
    BrowserOperation, OperationId, OperationSequence, RevisionId, SessionId, SubmissionId,
    ThoughtId, Timestamp,
};
use crate::ports::agent::{AgentState, SubmissionDisposition};

pub use browser_history::{BrowserCommitReceipt, BrowserHistoryEntry, BrowserHistoryStatus};
pub use capture::{CaptureCommit, CaptureCommitOutcome, CaptureReceipt};
pub use compaction::{
    CompactedOperationRequest, thought_payload_digest, thought_payload_digest_with_name,
};
pub use error::{StoreError, StoreFailureCode};
pub use migration::MigrationMode;
pub use onboarding::{FirstRunBoard, FirstRunOutcome, OnboardingVersion};
pub use operation::{OperationBatch, SemanticRequestFingerprint, StoredOperationRequest};
pub use receipt::{CommitReceipt, DurableIdentity};
pub use session::{SessionHit, SessionQuery, SessionSnapshot};
pub use session_request::{
    NamedSessionCreation, NamedSessionMatch, NamedSessionOutcome, NamedSessionPolicy,
    SessionRequest, SessionRequestReceipt, StoredSessionRequest, session_create_digest,
};
pub use submission_route::{SUBMISSION_ROUTE_VERSION, SubmissionJournalRoute};

/// Current storage schema understood by this binary.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 21;
/// Current local storage protocol understood by this binary.
pub const STORAGE_PROTOCOL_VERSION: u32 = 20;

/// One ordered, content-redacted source included in a submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmissionSource {
    /// Source thought.
    pub thought_id: ThoughtId,
    /// SHA-256 of the exact source content.
    pub source_digest: [u8; 32],
}

/// Durable lifecycle state for one content-redacted agent submission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionAttemptState {
    /// Intent is durable but external delivery has not started.
    Prepared,
    /// External delivery may be in progress.
    Sending,
    /// A matching semantic receipt established acceptance.
    Accepted,
    /// Delivery failed before acceptance.
    Failed,
    /// A prepared intent was abandoned before delivery.
    Cancelled,
    /// Proqi restarted after delivery began without a durable outcome.
    OutcomeUnknown,
}

impl SubmissionAttemptState {
    /// Stable representation used by SQLite and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Sending => "sending",
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }
}

/// Content-redacted durable submission record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionAttempt {
    /// Proqi submission identity.
    pub id: SubmissionId,
    /// Owning session.
    pub session_id: SessionId,
    /// Ordered source thoughts and their exact-content digests.
    pub sources: Vec<SubmissionSource>,
    /// SHA-256 of the complete concatenated prompt.
    pub payload_digest: [u8; 32],
    /// Latest durable source sequence when prepared.
    pub source_sequence: OperationSequence,
    /// Keep or remove after durable acceptance.
    pub disposition: SubmissionDisposition,
    /// Versioned content-redacted delivery route.
    pub route: SubmissionJournalRoute,
    /// Integration provider name.
    pub provider: String,
    /// Negotiated provider protocol.
    pub protocol: u32,
    /// SHA-256 fingerprint of target identity, never the raw identity.
    pub target_fingerprint: [u8; 32],
    /// Verified target state before delivery.
    pub pre_state: AgentState,
    /// Creation time.
    pub prepared_at: Timestamp,
}

/// Final durable fields for one submission attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionOutcome {
    /// Terminal attempt state.
    pub state: SubmissionAttemptState,
    /// Advisory harness state after acceptance.
    pub post_state: Option<AgentState>,
    /// Stable redacted failure code.
    pub error_code: Option<String>,
    /// Optional source deletion operation.
    pub deletion_operation_id: Option<OperationId>,
    /// Transition time.
    pub at: Timestamp,
}

/// Local durable store used by the TUI and CLI.
pub trait Store {
    /// Load a complete session snapshot.
    ///
    /// # Errors
    ///
    /// Returns a typed error for absence, incompatibility, corruption, or I/O failure.
    fn load_session(&mut self, id: SessionId) -> Result<SessionSnapshot, StoreError>;

    /// Compact retained history while the caller owns the session lease.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption, serialization, contention, or I/O failure.
    fn compact_session(&mut self, _id: SessionId) -> Result<(), StoreError> {
        Ok(())
    }

    /// Search current state without consulting canonical data outside SQLite.
    ///
    /// # Errors
    ///
    /// Returns a typed storage failure.
    fn search_sessions(&mut self, query: &SessionQuery) -> Result<Vec<SessionHit>, StoreError>;

    /// Record opening metadata after the caller acquired the session lease.
    ///
    /// # Errors
    ///
    /// Returns a typed absence, validation, or persistence failure.
    fn record_session_open(
        &mut self,
        id: SessionId,
        cwd: &std::path::Path,
        at: Timestamp,
    ) -> Result<(), StoreError>;

    /// Replace or clear an optional session name.
    ///
    /// # Errors
    ///
    /// Returns a typed absence, validation, or persistence failure.
    fn rename_session(&mut self, id: SessionId, name: Option<&str>) -> Result<(), StoreError>;

    /// Atomically apply and retain one cross-session Browser operation.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict, invariant, or persistence failure.
    fn commit_browser_operation(
        &mut self,
        _operation: &BrowserOperation,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        Err(StoreError::Integrity(
            "browser history is unavailable".to_owned(),
        ))
    }

    /// Atomically undo or redo one Browser operation.
    ///
    /// # Errors
    ///
    /// Returns a typed empty-history, conflict, or persistence failure.
    fn move_browser_history(
        &mut self,
        _operation_id: OperationId,
        _target: BrowserHistoryEntry,
        _undo: bool,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        Err(StoreError::Integrity(
            "browser history is unavailable".to_owned(),
        ))
    }

    /// Inspect Browser history without mutating it.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption or persistence failure.
    fn browser_history_status(&mut self) -> Result<BrowserHistoryStatus, StoreError> {
        Ok(BrowserHistoryStatus::default())
    }

    /// Durably reserve or replay one no-op Browser rename request.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption or persistence failure.
    fn commit_browser_noop_rename(
        &mut self,
        _operation_id: OperationId,
        _session_id: SessionId,
        _name: Option<&str>,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        Err(StoreError::Integrity(
            "browser history is unavailable".to_owned(),
        ))
    }

    /// Look up one retained Browser operation for validation and recovery.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption or persistence failure.
    fn browser_operation(
        &mut self,
        _operation_id: OperationId,
    ) -> Result<Option<BrowserOperation>, StoreError> {
        Ok(None)
    }

    /// Atomically create one named session under its name-collision policy.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, contention, or persistence failure.
    fn create_named_session(
        &mut self,
        _creation: &NamedSessionCreation,
    ) -> Result<NamedSessionOutcome, StoreError> {
        Err(StoreError::Integrity(
            "named session creation is unavailable".to_owned(),
        ))
    }

    /// Look up the request that already owns one session-administration identity.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption or persistence failure.
    fn session_request(
        &mut self,
        _operation_id: OperationId,
    ) -> Result<Option<StoredSessionRequest>, StoreError> {
        Ok(None)
    }

    /// Durably reserve one trash request for a session that is already trashed.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the session is live or the identity is reused.
    fn commit_noop_trash(
        &mut self,
        _operation_id: OperationId,
        _session_id: SessionId,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        Err(StoreError::Integrity(
            "session administration receipts are unavailable".to_owned(),
        ))
    }

    /// Permanently prune one trashed session and retain its request receipt.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the session is live, otherwise a typed storage failure.
    fn prune_session_request(
        &mut self,
        _session_id: SessionId,
        _operation_id: OperationId,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        Err(StoreError::Integrity(
            "session administration receipts are unavailable".to_owned(),
        ))
    }

    /// Look up a prior operation request for cross-process idempotency.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption or persistence failure.
    fn operation_request(
        &mut self,
        id: OperationId,
    ) -> Result<Option<StoredOperationRequest>, StoreError>;

    /// Look up a prior editor revision for cross-process idempotency.
    ///
    /// # Errors
    ///
    /// Returns a typed corruption or persistence failure.
    fn revision_request(
        &mut self,
        id: RevisionId,
    ) -> Result<Option<StoredOperationRequest>, StoreError>;

    /// Atomically create a fresh session and claim the current onboarding version when eligible.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict, corruption, busy, integrity, or persistence failure.
    fn create_first_run_session(
        &mut self,
        board: &FirstRunBoard,
    ) -> Result<FirstRunOutcome, StoreError>;

    /// Atomically apply one operation batch.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict, busy, integrity, or I/O failure.
    fn commit(&mut self, batch: &OperationBatch) -> Result<Option<CommitReceipt>, StoreError>;

    /// Atomically create one screenshot thought and its installation-wide receipt.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict, busy, integrity, or I/O failure with no partial thought.
    fn commit_capture(
        &mut self,
        _capture: &CaptureCommit,
    ) -> Result<CaptureCommitOutcome, StoreError> {
        Err(StoreError::Integrity(
            "screenshot capture receipts are unavailable".to_owned(),
        ))
    }

    /// Durably reserve one thought for an external submission.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the thought already has an active attempt, or a typed storage error.
    fn prepare_submission(&mut self, _attempt: &SubmissionAttempt) -> Result<(), StoreError> {
        Err(StoreError::Integrity(
            "submission journal is unavailable".to_owned(),
        ))
    }

    /// Compare and set one prepared submission to sending.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the attempt is not prepared, or a typed storage error.
    fn mark_submission_sending(
        &mut self,
        _id: SubmissionId,
        _at: Timestamp,
    ) -> Result<(), StoreError> {
        Err(StoreError::Integrity(
            "submission journal is unavailable".to_owned(),
        ))
    }

    /// Compare and set one sending submission to a terminal outcome.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the attempt is not sending, or a typed storage error.
    fn finish_submission(
        &mut self,
        _id: SubmissionId,
        _outcome: &SubmissionOutcome,
    ) -> Result<(), StoreError> {
        Err(StoreError::Integrity(
            "submission journal is unavailable".to_owned(),
        ))
    }

    /// Atomically record an accepted outcome and its source-removal operation.
    ///
    /// # Errors
    ///
    /// Returns a typed error with neither change committed.
    fn finish_submission_with_removal(
        &mut self,
        _id: SubmissionId,
        _outcome: &SubmissionOutcome,
        _removal: &crate::domain::BoardOperation,
    ) -> Result<CommitReceipt, StoreError> {
        Err(StoreError::Integrity(
            "atomic submission removal is unavailable".to_owned(),
        ))
    }

    /// Recover incomplete attempts only after acquiring their session lease.
    ///
    /// # Errors
    ///
    /// Returns a typed storage error without retrying any ambiguous delivery.
    fn recover_submissions(
        &mut self,
        _session_id: SessionId,
        _at: Timestamp,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    /// Move a session to recoverable trash.
    ///
    /// # Errors
    ///
    /// Returns a typed storage failure.
    fn trash_session(&mut self, id: SessionId, at: Timestamp) -> Result<(), StoreError>;

    /// Restore a recoverably trashed session.
    ///
    /// # Errors
    ///
    /// Returns a typed storage failure.
    fn restore_session(&mut self, id: SessionId) -> Result<(), StoreError>;

    /// Permanently prune one already trashed session.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the session is live, otherwise a typed storage failure.
    fn prune_session(&mut self, id: SessionId) -> Result<(), StoreError>;
}
