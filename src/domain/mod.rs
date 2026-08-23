//! Entities, value objects, identifiers, and invariants.

pub mod identifiers;
pub mod model;
pub mod operations;

pub use identifiers::{
    InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
};
pub use model::{
    Direction, DomainError, IntegrationContext, OperationRecord, OperationSequence, Session,
    Thought, ThoughtPosition, ThoughtRevision, Timestamp,
};
pub use operations::{BoardMutation, BoardOperation, BoardOperationKind, SessionBoard, UndoScope};
