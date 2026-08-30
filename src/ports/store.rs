//! Persistence facade expressed in domain terms.

mod compaction;
mod error;
mod onboarding;

use serde::{Deserialize, Serialize};

use crate::domain::{
    BoardOperation, Direction, IntegrationContext, OperationId, OperationSequence, RevisionId,
    Session, SessionBoard, SessionId, SubmissionId, ThoughtId, ThoughtRevision, Timestamp,
    UndoScope,
};
use crate::ports::agent::{AgentState, SubmissionDisposition};
use crate::ports::screenshot::ScreenshotFingerprint;

pub use compaction::{CompactedOperationRequest, thought_payload_digest};
pub use error::{StoreError, StoreFailureCode};
pub use onboarding::{FirstRunBoard, FirstRunOutcome, OnboardingVersion};

/// Current storage schema understood by this binary.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 9;
/// Current local storage protocol understood by this binary.
pub const STORAGE_PROTOCOL_VERSION: u32 = 9;

/// One atomic screenshot receipt and prospective board operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureCommit {
    /// Rename-stable source identity.
    pub source: ScreenshotFingerprint,
    /// Exact append operation, applied only with the receipt.
    pub operation: BoardOperation,
}

/// Durable identity of one screenshot already delivered to a session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureReceipt {
    /// Rename-stable source identity.
    pub source: ScreenshotFingerprint,
    /// Session that received the screenshot.
    pub session_id: SessionId,
    /// Thought created for the screenshot.
    pub thought_id: ThoughtId,
    /// Structural operation that created the thought.
    pub operation_id: OperationId,
    /// Commit timestamp.
    pub accepted_at: Timestamp,
}

/// Atomic screenshot commit result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureCommitOutcome {
    /// The receipt and thought were created together.
    Created {
        /// Ordinary board durability receipt.
        durable: CommitReceipt,
        /// Durable capture receipt.
        capture: CaptureReceipt,
    },
    /// This source had already been delivered by an earlier owner or retry.
    AlreadyCaptured(CaptureReceipt),
}

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
    /// Adjacent target direction.
    pub direction: Direction,
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

/// Whether this process proved it holds the exclusive schema lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationMode {
    /// Migrations may run after backup and integrity checks.
    Allow,
    /// Opening an older schema fails without modifying it.
    Refuse,
}

/// One commit accepted durably by the store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitReceipt {
    /// Owning session.
    pub session_id: SessionId,
    /// Monotonic commit sequence.
    pub sequence: OperationSequence,
    /// Durable entity used for idempotency.
    pub identity: DurableIdentity,
    /// Whether this exact commit had already succeeded.
    pub idempotent_replay: bool,
}

/// Typed identity of a durable commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum DurableIdentity {
    /// Structural or history movement operation.
    Operation(OperationId),
    /// Editor revision.
    Revision(RevisionId),
}

/// Previously committed request associated with a durable operation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredOperationRequest {
    /// Reversible board mutation.
    Board {
        /// Original operation payload.
        operation: Box<BoardOperation>,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
    /// Persistent undo or redo request.
    HistoryMove {
        /// Owning session.
        session_id: SessionId,
        /// Addressed history scope.
        scope: UndoScope,
        /// Undo when true, redo when false.
        undo: bool,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
    /// Exact editor replacement revision.
    Revision {
        /// Original editor revision.
        revision: Box<ThoughtRevision>,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
    /// Content-redacted semantic replay data retained after history compaction.
    Compacted {
        /// Minimal fields required to compare a replay safely.
        replay: CompactedOperationRequest,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
}

/// One atomic persistence request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationBatch {
    /// Insert a new session.
    CreateSession(Session),
    /// Apply and retain one reversible board operation.
    Board(BoardOperation),
    /// Apply and retain one reversible editor revision.
    Revision(ThoughtRevision),
    /// Move one persistent undo or redo cursor.
    HistoryMove {
        /// Idempotent durable operation identity.
        operation_id: OperationId,
        /// Owning session.
        session_id: SessionId,
        /// Board or one thought's editor history.
        scope: UndoScope,
        /// Undo when true, redo when false.
        undo: bool,
        /// Next monotonic sequence.
        sequence: OperationSequence,
        /// Event time.
        at: Timestamp,
    },
    /// Store recognition-only integration context.
    IntegrationContext {
        /// Owning session.
        session_id: SessionId,
        /// New context, or `None` to clear it.
        context: Option<IntegrationContext>,
    },
}

impl OperationBatch {
    /// Return the ordered session sequence carried by a mutable operation.
    #[must_use]
    pub const fn sequence(&self) -> Option<OperationSequence> {
        match self {
            Self::Board(operation) => Some(operation.sequence),
            Self::Revision(revision) => Some(revision.sequence),
            Self::HistoryMove { sequence, .. } => Some(*sequence),
            Self::CreateSession(_) | Self::IntegrationContext { .. } => None,
        }
    }
}

/// Complete persisted session state and reversible history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    /// Current validated board.
    pub board: SessionBoard,
    /// Structural history in application order.
    pub board_operations: Vec<BoardOperation>,
    /// Applied prefix length of structural history.
    pub board_history_cursor: usize,
    /// Editor revisions retained for every thought.
    pub revisions: Vec<ThoughtRevision>,
    /// Applied editor prefix length for every thought.
    pub editor_history_cursors: Vec<(ThoughtId, usize)>,
    /// Last verified recognition-only integration context.
    pub integration_context: Option<IntegrationContext>,
}

/// Search options for the session browser and CLI.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionQuery {
    /// Optional words matched across names, paths, and current thought content.
    pub text: Option<String>,
    /// Whether recoverably trashed sessions are included.
    pub include_trashed: bool,
    /// Optional current directory used only for ranking.
    pub current_directory: Option<std::path::PathBuf>,
}

/// Lightweight session search result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionHit {
    /// Stable session identity.
    pub id: SessionId,
    /// Optional session name.
    pub name: Option<String>,
    /// Directory in which the session was first created.
    pub origin_cwd: std::path::PathBuf,
    /// Directory from which it was last opened.
    pub last_opened_cwd: std::path::PathBuf,
    /// Latest successful opening time.
    pub last_opened_at: Timestamp,
    /// Latest activity time.
    pub last_active_at: Timestamp,
    /// Number of live thoughts.
    pub thought_count: usize,
    /// Derived first useful content excerpt.
    pub excerpt: String,
    /// First two useful exact-content previews, each independently bounded.
    pub previews: Vec<String>,
    /// Complete live thought corpus used only by the in-memory browser filter.
    #[serde(skip)]
    pub search_content: String,
    /// Last verified adjacent-agent recognition context.
    pub integration_context: Option<IntegrationContext>,
    /// Whether the session is in recoverable trash.
    pub trashed: bool,
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
