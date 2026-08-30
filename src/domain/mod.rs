//! Entities, value objects, identifiers, and invariants.

mod annotations;
mod identifiers;
mod model;
mod operations;
mod text;
mod update;

pub use annotations::{
    AnnotationTextChange, extract_annotations, merge_annotations, partition_annotations,
    rebase_annotations, validate_annotations,
};
pub use identifiers::{
    InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
};
pub use model::{
    ContentAnnotation, ContentAnnotationKind, Direction, DomainError, IntegrationContext,
    OperationSequence, Session, Thought, ThoughtPosition, ThoughtPresentation, ThoughtRevision,
    Timestamp,
};
pub use operations::{
    BoardMutation, BoardOperation, BoardOperationKind, OperationRecord, SessionBoard, UndoScope,
};
pub use text::TextPosition;
pub use update::{
    Installation, InstallationIdentity, InstallationKind, StableVersion, UpdateCacheState,
    UpdateValueError,
};
