//! Entities, value objects, identifiers, and invariants.

mod annotation;
mod attachment_numbering;
mod browser_history;
mod identifiers;
mod model;
mod operations;
mod release_highlights;
mod text;
mod update;

pub use annotation::{
    AnnotationBehavior, AnnotationTextChange, ContentAnnotation, ContentAnnotationKind,
    InlineStyleKind, ShortcutEmphasis, extract_annotations, merge_annotations,
    partition_annotations, rebase_annotations, validate_annotations,
};
pub use attachment_numbering::{
    AttachmentCounters, AttachmentOrdinal, renew_attachment_occurrences,
};
pub use browser_history::{BrowserMutation, BrowserOperation, BrowserOperationKind};
pub use identifiers::{
    InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
};
pub use model::{
    Direction, DomainError, IntegrationContext, OperationSequence, Session, Thought,
    ThoughtPosition, ThoughtPresentation, ThoughtRevision, Timestamp,
};
pub use operations::{
    BoardMutation, BoardOperation, BoardOperationKind, OperationRecord, SessionBoard, UndoScope,
};
pub use release_highlights::{
    RELEASE_HIGHLIGHT_MAX_CHARS, RELEASE_HIGHLIGHTS_MAX_BYTES, RELEASE_HIGHLIGHTS_MAX_ITEMS,
    RELEASE_HIGHLIGHTS_MIN_ITEMS, ReleaseHighlightAnnouncement, ReleaseHighlightAnnouncementError,
    ReleaseHighlightGroup, ReleaseHighlightsError, ReleaseHighlightsManifest,
};
pub use text::TextPosition;
pub use update::{
    Installation, InstallationIdentity, InstallationKind, StableVersion, UpdateCacheState,
    UpdateValueError,
};
