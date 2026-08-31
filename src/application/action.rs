//! Normalized reducer inputs from terminal and control boundaries.

use super::FailureCode;
use crate::domain::{
    BoardOperationKind, ContentAnnotation, OperationId, OperationSequence, RequestId, RevisionId,
    TextPosition, ThoughtId, ThoughtPresentation, Timestamp, UndoScope,
};

/// Normalized input or external result accepted by the reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Replace or clear the current session's optional name.
    RenameSession {
        /// New validated name, or `None` to clear it.
        name: Option<String>,
    },
    /// Focus one live thought, or clear focus.
    FocusThought(Option<ThoughtId>),
    /// Enter the multiline editor for one live thought.
    EnterEdit(ThoughtId),
    /// Enter the transient insertion editor without creating durable state.
    EnterCompose,
    /// Return from transient composition to the empty board.
    ExitCompose,
    /// Return from edit mode to the board.
    ExitEdit,
    /// Create a blank or pre-populated thought as one operation.
    CreateThought {
        /// New thought identity.
        thought_id: ThoughtId,
        /// Durable operation identity.
        operation_id: OperationId,
        /// Exact initial content.
        content: String,
        /// Durable presentation metadata over the exact initial content.
        annotations: Vec<ContentAnnotation>,
        /// Explicit insertion point, or the current insertion point.
        insertion_index: Option<usize>,
        /// Event time.
        at: Timestamp,
    },
    /// Board-mode paste, intentionally equivalent to one create operation.
    PasteAsThought {
        /// New thought identity.
        thought_id: ThoughtId,
        /// Durable operation identity.
        operation_id: OperationId,
        /// Exact bracketed-paste payload.
        content: String,
        /// Durable presentation metadata over the exact paste payload.
        annotations: Vec<ContentAnnotation>,
        /// Event time.
        at: Timestamp,
    },
    /// Apply one exact editor revision.
    EditThought {
        /// Edited thought.
        thought_id: ThoughtId,
        /// Stable revision identity.
        revision_id: RevisionId,
        /// Required current content.
        before_content: String,
        /// Replacement content.
        after_content: String,
        /// Presentation metadata required before the edit.
        before_annotations: Vec<ContentAnnotation>,
        /// Replacement presentation metadata.
        after_annotations: Vec<ContentAnnotation>,
        /// Cursor before the edit.
        before_cursor: TextPosition,
        /// Cursor after the edit.
        after_cursor: TextPosition,
        /// Event time.
        at: Timestamp,
    },
    /// Request exact-content copy of thoughts in board order.
    CopyThoughts {
        /// Idempotent external request identity.
        request_id: RequestId,
        /// Ordered source thoughts.
        thought_ids: Vec<ThoughtId>,
    },
    /// Request exact-content cut, deferring deletion until clipboard success.
    CutThoughts {
        /// Idempotent external request identity.
        request_id: RequestId,
        /// Durable operation used only after success.
        operation_id: OperationId,
        /// Ordered source thoughts.
        thought_ids: Vec<ThoughtId>,
        /// Event time.
        at: Timestamp,
    },
    /// Clipboard adapter result.
    ClipboardResult {
        /// Matching request.
        request_id: RequestId,
        /// Success or stable failure code.
        result: Result<(), FailureCode>,
    },
    /// Lock source thoughts from submission intent through journaled outcome.
    BeginSubmission {
        /// Ordered source thoughts.
        thought_ids: Vec<ThoughtId>,
    },
    /// Release source thoughts after a terminal submission outcome.
    EndSubmission {
        /// Ordered source thoughts.
        thought_ids: Vec<ThoughtId>,
    },
    /// Soft-delete without touching the clipboard.
    DeleteThought {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Thought to delete.
        thought_id: ThoughtId,
        /// Semantic deletion kind.
        kind: BoardOperationKind,
        /// Event time.
        at: Timestamp,
    },
    /// Soft-delete several thoughts as one board-history operation.
    DeleteThoughts {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Thoughts to delete in board order.
        thought_ids: Vec<ThoughtId>,
        /// Semantic deletion kind.
        kind: BoardOperationKind,
        /// Event time.
        at: Timestamp,
    },
    /// Reserve a submission removal without changing the visible board before durability.
    StageSubmissionRemoval {
        /// Durable operation identity recorded with the accepted submission.
        operation_id: OperationId,
        /// Submission sources to remove atomically in board order.
        thought_ids: Vec<ThoughtId>,
        /// Event time.
        at: Timestamp,
    },
    /// Reorder one live thought.
    MoveThought {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Thought to move.
        thought_id: ThoughtId,
        /// Desired zero-based live position.
        to: usize,
        /// Event time.
        at: Timestamp,
    },
    /// Set the durable presentation preference.
    SetPresentation {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Affected thought.
        thought_id: ThoughtId,
        /// New preference.
        presentation: ThoughtPresentation,
        /// Event time.
        at: Timestamp,
    },
    /// Set one presentation preference as one board-history operation.
    SetPresentationMany {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Thoughts to update in board order.
        thought_ids: Vec<ThoughtId>,
        /// New preference.
        presentation: ThoughtPresentation,
        /// Event time.
        at: Timestamp,
    },
    /// Duplicate one or more thoughts in board order as one operation.
    DuplicateThoughts {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Source thoughts in board order.
        thought_ids: Vec<ThoughtId>,
        /// Fresh identities paired with the ordered sources.
        duplicate_ids: Vec<ThoughtId>,
        /// Event time.
        at: Timestamp,
    },
    /// Persistently undo one board operation or editor revision.
    Undo {
        /// Idempotent durable control identity.
        operation_id: OperationId,
        /// Explicit history scope.
        scope: UndoScope,
        /// Event time.
        at: Timestamp,
    },
    /// Persistently redo one board operation or editor revision.
    Redo {
        /// Idempotent durable control identity.
        operation_id: OperationId,
        /// Explicit history scope.
        scope: UndoScope,
        /// Event time.
        at: Timestamp,
    },
    /// Storage acknowledged one ordered sequence.
    PersistenceCommitted(OperationSequence),
    /// Storage failed one ordered sequence.
    PersistenceFailed {
        /// Failed sequence.
        sequence: OperationSequence,
        /// Stable failure class.
        code: FailureCode,
    },
    /// Ask the storage lane to retry one retained failed operation.
    RetryPersistence(OperationSequence),
}
