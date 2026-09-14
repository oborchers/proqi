//! Store-facing receipts and availability for durable Browser history.

use crate::domain::{BrowserOperationKind, OperationId, SessionId};

/// Durable result of a Browser operation or Browser history movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserCommitReceipt {
    /// Stable request identity.
    pub operation_id: OperationId,
    /// Browser cursor after the commit.
    pub cursor: usize,
    /// Whether the exact request had already committed.
    pub idempotent_replay: bool,
}

/// History availability and truthful labels for the session browser.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BrowserHistoryStatus {
    /// Semantic entry that an undo would reverse.
    pub undo: Option<BrowserHistoryEntry>,
    /// Semantic entry that a redo would reapply.
    pub redo: Option<BrowserHistoryEntry>,
}

/// Exact Browser history entry used for availability and race-safe movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserHistoryEntry {
    /// Stable identity of the operation being moved.
    pub operation_id: OperationId,
    /// Session resource whose lease must protect the movement.
    pub session_id: SessionId,
    /// Semantic operation used for truthful presentation.
    pub kind: BrowserOperationKind,
}
