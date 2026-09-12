//! Stable receipts for session-scoped durable commits.

use serde::{Deserialize, Serialize};

use crate::domain::{OperationId, OperationSequence, RevisionId, SessionId};

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
