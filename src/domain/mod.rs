//! Entities, value objects, identifiers, and invariants.

mod identifiers;
mod model;
mod operations;
mod text;

pub use identifiers::{
    InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
};
pub use model::{
    Direction, DomainError, IntegrationContext, OperationRecord, OperationSequence, Session,
    Thought, ThoughtPosition, ThoughtRevision, Timestamp,
};
pub use operations::{BoardMutation, BoardOperation, BoardOperationKind, SessionBoard, UndoScope};
pub use text::TextPosition;
