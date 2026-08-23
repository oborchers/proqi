//! Persistence facade expressed in domain terms.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    BoardOperation, IntegrationContext, OperationId, OperationSequence, RevisionId, Session,
    SessionBoard, SessionId, ThoughtId, ThoughtRevision, Timestamp, UndoScope,
};

/// Current storage schema understood by this binary.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;
/// Current local storage protocol understood by this binary.
pub const STORAGE_PROTOCOL_VERSION: u32 = 1;

/// Whether this process proved it holds the exclusive schema lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationMode {
    /// Migrations may run after backup and integrity checks.
    Allow,
    /// Opening an older schema fails without modifying it.
    Refuse,
}

/// One commit accepted durably by the store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurableIdentity {
    /// Structural or history movement operation.
    Operation(OperationId),
    /// Editor revision.
    Revision(RevisionId),
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
    /// Directory from which it was last opened.
    pub last_opened_cwd: std::path::PathBuf,
    /// Latest activity time.
    pub last_active_at: Timestamp,
    /// Number of live thoughts.
    pub thought_count: usize,
    /// Derived first useful content excerpt.
    pub excerpt: String,
    /// Whether the session is in recoverable trash.
    pub trashed: bool,
}

/// Typed persistence failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StoreError {
    /// SQLite writer remained contended after bounded retries.
    #[error("storage is busy")]
    Busy,
    /// A requested durable record does not exist.
    #[error("storage record not found: {0}")]
    NotFound(String),
    /// Current state does not satisfy a commit precondition.
    #[error("storage conflict: {0}")]
    Conflict(String),
    /// Database or index integrity validation failed.
    #[error("storage integrity check failed: {0}")]
    Integrity(String),
    /// Database schema is newer than this binary.
    #[error("unsupported storage schema {found}, maximum supported is {supported}")]
    UnsupportedSchema {
        /// Schema found on disk.
        found: u32,
        /// Maximum supported schema.
        supported: u32,
    },
    /// Storage protocol is newer even though the table schema is recognized.
    #[error("unsupported storage protocol {found}, maximum supported is {supported}")]
    UnsupportedStorageProtocol {
        /// Protocol found on disk.
        found: u32,
        /// Maximum supported protocol.
        supported: u32,
    },
    /// Schema is older but this process lacks exclusive migration authority.
    #[error("storage schema {found} requires migration to {supported}")]
    MigrationRequired {
        /// Schema found on disk.
        found: u32,
        /// Required schema.
        supported: u32,
    },
    /// A pre-migration backup could not be completed.
    #[error("storage backup failed: {0}")]
    Backup(String),
    /// Database contents or identifiers are malformed.
    #[error("storage is corrupt or malformed: {0}")]
    Corrupt(String),
    /// Filesystem operation failed.
    #[error("storage I/O failed: {0}")]
    Io(String),
    /// JSON operation payload could not be encoded or decoded.
    #[error("storage serialization failed: {0}")]
    Serialization(String),
    /// A validated domain invariant rejected persisted state.
    #[error("stored domain invariant failed: {0}")]
    Invariant(String),
    /// Available storage could not accept a write.
    #[error("storage device is full")]
    DiskFull,
}

/// Local durable store used by the TUI and CLI.
pub trait Store {
    /// Load a complete session snapshot.
    ///
    /// # Errors
    ///
    /// Returns a typed error for absence, incompatibility, corruption, or I/O failure.
    fn load_session(&mut self, id: SessionId) -> Result<SessionSnapshot, StoreError>;

    /// Search current state without consulting canonical data outside SQLite.
    ///
    /// # Errors
    ///
    /// Returns a typed storage failure.
    fn search_sessions(&mut self, query: &SessionQuery) -> Result<Vec<SessionHit>, StoreError>;

    /// Atomically apply one operation batch.
    ///
    /// # Errors
    ///
    /// Returns a typed conflict, busy, integrity, or I/O failure.
    fn commit(&mut self, batch: &OperationBatch) -> Result<Option<CommitReceipt>, StoreError>;

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
