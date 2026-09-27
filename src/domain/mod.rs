//! Entities, value objects, identifiers, and invariants.

mod annotation;
mod attachment_numbering;
mod board_item;
mod browser_history;
mod export;
mod identifiers;
mod model;
mod operations;
mod release_highlights;
mod text;
mod thought_name;
mod update;

pub use annotation::{
    AnnotationBehavior, AnnotationTextChange, ContentAnnotation, ContentAnnotationKind,
    InlineStyleKind, ShortcutEmphasis, extract_annotations, merge_annotations,
    partition_annotations, rebase_annotations, validate_annotations,
};
pub use attachment_numbering::{
    AttachmentCounters, AttachmentOrdinal, renew_attachment_occurrences,
};
pub use board_item::{BoardItemId, BoardItemRef, Separator};
pub use browser_history::{BrowserMutation, BrowserOperation, BrowserOperationKind};
pub use export::{
    EXPORT_EXTENSION, EXPORT_STEM_MAX_BYTES, ExportDisposition, ExportPathError,
    default_export_file_name, resolve_export_path, sanitize_file_stem, utc_file_timestamp,
};
pub use identifiers::{
    InstanceId, OperationId, RequestId, RevisionId, SeparatorId, SessionId, SubmissionId, ThoughtId,
};
pub use model::{
    Direction, DomainError, IntegrationContext, OperationSequence, Session, Thought,
    ThoughtPosition, ThoughtPresentation, ThoughtRevision, Timestamp, validate_session_name,
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
pub use thought_name::{THOUGHT_NAME_MAX_CHARS, ThoughtName};
pub use update::{
    EXTERNAL_RESTART_MAX_EXPECTATIONS, ExternalRestartExpectation, ExternalRestartPending,
    Installation, InstallationIdentity, InstallationKind, InstalledVersionRelation, StableVersion,
    UpdateCacheState, UpdateValueError,
};
