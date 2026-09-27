//! Durable Board and editor operation request contracts.

use crate::domain::{
    BoardOperation, IntegrationContext, OperationId, OperationSequence, Session, SessionId,
    ThoughtId, ThoughtRevision, Timestamp, UndoScope,
};

use super::{CommitReceipt, CompactedOperationRequest, DurableIdentity, StoreError};

/// Content-redacted identity of one exact public mutation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticRequestFingerprint([u8; 32]);

impl SemanticRequestFingerprint {
    /// Construct a fingerprint from its complete SHA-256 bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the complete SHA-256 bytes for durable storage.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Previously committed request associated with a durable operation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredOperationRequest {
    /// Reversible board mutation.
    Board {
        /// Original operation payload.
        operation: Box<BoardOperation>,
        /// Exact public request identity, when this operation came from the API.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
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
        /// Exact public request identity, when this operation came from the API.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
    /// Exact editor replacement revision.
    Revision {
        /// Original editor revision.
        revision: Box<ThoughtRevision>,
        /// Exact public request identity, when this revision came from the API.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
    /// Content-redacted semantic replay data retained after history compaction.
    Compacted {
        /// Minimal fields required to compare a replay safely.
        replay: CompactedOperationRequest,
        /// Exact public request identity retained independently of history payloads.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
        /// Original durable receipt.
        receipt: CommitReceipt,
    },
    /// A session-administration request already owns the identity.
    ///
    /// It never matches a Board, editor, or history mutation, so any such reuse
    /// is an idempotency conflict rather than a storage conflict.
    SessionAdministration,
}

impl StoredOperationRequest {
    /// Return the exact public request identity retained with this receipt.
    #[must_use]
    pub const fn semantic_fingerprint(&self) -> Option<SemanticRequestFingerprint> {
        match self {
            Self::Board {
                semantic_fingerprint,
                ..
            }
            | Self::HistoryMove {
                semantic_fingerprint,
                ..
            }
            | Self::Revision {
                semantic_fingerprint,
                ..
            }
            | Self::Compacted {
                semantic_fingerprint,
                ..
            } => *semantic_fingerprint,
            Self::SessionAdministration => None,
        }
    }
}

/// One atomic persistence request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationBatch {
    /// Insert a new session.
    CreateSession(Session),
    /// Apply and retain one reversible board operation.
    Board {
        /// Reversible Board operation.
        operation: BoardOperation,
        /// Exact public request identity, or `None` for non-API mutations.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
    },
    /// Apply and retain one reversible editor revision.
    Revision {
        /// Reversible editor revision.
        revision: ThoughtRevision,
        /// Exact public request identity, or `None` for non-API mutations.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
    },
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
        /// Exact public request identity, or `None` for non-API mutations.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
    },
    /// Reserve a retry-safe same-value thought rename without adding history.
    ThoughtNoOpRename {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Owning session.
        session_id: SessionId,
        /// Thought whose current name must match.
        thought_id: ThoughtId,
        /// Current and requested name.
        name: Option<crate::domain::ThoughtName>,
        /// Next monotonic sequence.
        sequence: OperationSequence,
        /// Event time.
        at: Timestamp,
        /// Exact public request identity, or `None` for non-API mutations.
        semantic_fingerprint: Option<SemanticRequestFingerprint>,
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
            Self::Board { operation, .. } => Some(operation.sequence),
            Self::Revision { revision, .. } => Some(revision.sequence),
            Self::HistoryMove { sequence, .. } | Self::ThoughtNoOpRename { sequence, .. } => {
                Some(*sequence)
            }
            Self::CreateSession(_) | Self::IntegrationContext { .. } => None,
        }
    }

    /// Attach one exact public request identity to a compatible durable batch.
    ///
    /// # Errors
    ///
    /// Returns a conflict when the batch identity differs from the request identity.
    pub fn attach_semantic_fingerprint(
        &mut self,
        identity: DurableIdentity,
        fingerprint: SemanticRequestFingerprint,
    ) -> Result<(), StoreError> {
        let destination = match self {
            Self::Board {
                operation,
                semantic_fingerprint,
            } if identity == DurableIdentity::Operation(operation.id) => semantic_fingerprint,
            Self::Revision {
                revision,
                semantic_fingerprint,
            } if identity == DurableIdentity::Revision(revision.id) => semantic_fingerprint,
            Self::HistoryMove {
                operation_id,
                semantic_fingerprint,
                ..
            }
            | Self::ThoughtNoOpRename {
                operation_id,
                semantic_fingerprint,
                ..
            } if identity == DurableIdentity::Operation(*operation_id) => semantic_fingerprint,
            Self::CreateSession(_)
            | Self::Board { .. }
            | Self::Revision { .. }
            | Self::HistoryMove { .. }
            | Self::ThoughtNoOpRename { .. }
            | Self::IntegrationContext { .. } => {
                return Err(StoreError::Conflict(
                    "semantic request identity does not match its durable batch".to_owned(),
                ));
            }
        };
        *destination = Some(fingerprint);
        Ok(())
    }
}
