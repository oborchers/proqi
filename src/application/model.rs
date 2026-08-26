//! Application state, normalized actions, effects, and errors.

mod effect;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::error::{ApplicationError, ApplicationResult, FailureCode};
use crate::domain::{
    BoardOperation, BoardOperationKind, ContentAnnotation, OperationId, OperationSequence,
    RequestId, RevisionId, SessionBoard, StableVersion, TextPosition, Thought, ThoughtId,
    ThoughtRevision, Timestamp, UndoScope,
};

pub use effect::Effect;

/// Active interaction context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionMode {
    /// Navigate and operate on whole thoughts.
    Board,
    /// Edit the focused thought.
    Edit {
        /// Thought being edited.
        thought_id: ThoughtId,
    },
}

/// Durability state shown truthfully by the interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurabilityState {
    /// No operation is awaiting storage acknowledgement.
    Durable {
        /// Latest acknowledged sequence.
        sequence: OperationSequence,
    },
    /// One or more operations await acknowledgement.
    Pending {
        /// Latest acknowledged sequence.
        durable: OperationSequence,
        /// Highest pending sequence.
        latest: OperationSequence,
    },
    /// A write failed while the in-memory state remains available.
    Failed {
        /// Latest acknowledged sequence.
        durable: OperationSequence,
        /// Sequence that failed.
        failed: OperationSequence,
        /// Stable error code.
        code: FailureCode,
    },
}

/// Clipboard purpose, kept separate from terminal keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardIntent {
    /// Preserve the thought after writing exact content.
    Copy,
    /// Delete only after a successful write.
    Cut,
}

/// Explicit user decision from an elected installation-wide update prompt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateIntent {
    /// Coordinate one verified Homebrew upgrade and restart all compatible sessions.
    Install(StableVersion),
    /// Defer this exact version until a later stale refresh.
    Dismiss(StableVersion),
    /// Suppress this exact version until a newer stable version exists.
    Skip(StableVersion),
    /// Show accurate standalone replacement instructions.
    ViewInstructions(StableVersion),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingClipboard {
    pub(super) thought_id: ThoughtId,
    pub(super) intent: ClipboardIntent,
    pub(super) operation_id: Option<OperationId>,
    pub(super) at: Timestamp,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct EditorHistory {
    pub(super) revisions: Vec<ThoughtRevision>,
    pub(super) cursor: usize,
}

/// Complete mutable state owned by the reducer lane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppState {
    /// Validated current board.
    pub board: SessionBoard,
    /// Current board or editor context.
    pub mode: InteractionMode,
    /// Focused live thought, if any.
    pub focused_thought: Option<ThoughtId>,
    /// Current insertion position used for new thoughts.
    pub insertion_index: usize,
    /// Current durability status.
    pub durability: DurabilityState,
    pub(super) board_history: Vec<BoardOperation>,
    pub(super) board_history_cursor: usize,
    pub(super) editor_histories: HashMap<ThoughtId, EditorHistory>,
    pub(super) pending_clipboard: BTreeMap<RequestId, PendingClipboard>,
    pub(super) pending_sequences: BTreeSet<OperationSequence>,
    pub(super) highest_sequence: OperationSequence,
}

impl AppState {
    /// Construct application state from a validated session snapshot.
    #[must_use]
    pub fn new(board: SessionBoard) -> Self {
        let focused_thought = board.live_thoughts().first().map(|thought| thought.id);
        let insertion_index = board.live_thoughts().len();
        let sequence = board.session.last_durable_sequence;
        Self {
            board,
            mode: InteractionMode::Board,
            focused_thought,
            insertion_index,
            durability: DurabilityState::Durable { sequence },
            board_history: Vec::new(),
            board_history_cursor: 0,
            editor_histories: HashMap::new(),
            pending_clipboard: BTreeMap::new(),
            pending_sequences: BTreeSet::new(),
            highest_sequence: sequence,
        }
    }

    /// Board history entries retained for undo and redo.
    #[must_use]
    pub fn board_history(&self) -> &[BoardOperation] {
        &self.board_history
    }

    /// Number of currently applied board history entries.
    #[must_use]
    pub const fn board_history_cursor(&self) -> usize {
        self.board_history_cursor
    }

    /// Number of currently applied revisions for one thought.
    #[must_use]
    pub fn editor_history_cursor(&self, thought_id: ThoughtId) -> usize {
        self.editor_histories
            .get(&thought_id)
            .map_or(0, |history| history.cursor)
    }

    /// Restore the logical cursor represented by the currently applied revision prefix.
    #[must_use]
    pub fn restored_editor_cursor(&self, thought_id: ThoughtId) -> Option<TextPosition> {
        let history = self.editor_histories.get(&thought_id)?;
        if history.cursor == 0 {
            history
                .revisions
                .first()
                .map(|revision| revision.before_cursor)
        } else {
            history
                .revisions
                .get(history.cursor - 1)
                .map(|revision| revision.after_cursor)
        }
    }

    pub(super) fn next_sequence(&self) -> ApplicationResult<OperationSequence> {
        self.highest_sequence
            .checked_next()
            .ok_or(ApplicationError::SequenceExhausted)
    }

    pub(super) fn track_pending(&mut self, sequence: OperationSequence) {
        self.highest_sequence = sequence;
        self.pending_sequences.insert(sequence);
        self.refresh_durability();
    }

    pub(super) fn refresh_durability(&mut self) {
        let durable = self.board.session.last_durable_sequence;
        if matches!(
            self.durability,
            DurabilityState::Failed { failed, .. } if self.pending_sequences.contains(&failed)
        ) {
            return;
        }
        self.durability = self.pending_sequences.last().map_or(
            DurabilityState::Durable { sequence: durable },
            |latest| DurabilityState::Pending {
                durable,
                latest: *latest,
            },
        );
    }

    pub(super) fn live_thought(&self, id: ThoughtId) -> ApplicationResult<&Thought> {
        self.board
            .thought(id)
            .filter(|thought| thought.is_live())
            .ok_or(ApplicationError::ThoughtNotFound(id))
    }

    pub(super) fn record_board_operation(
        &mut self,
        operation: &BoardOperation,
    ) -> ApplicationResult<()> {
        let mut board = self.board.clone();
        board.apply_mutation(&operation.forward, operation.created_at)?;
        self.board = board;
        self.board_history.truncate(self.board_history_cursor);
        self.board_history.push(operation.clone());
        self.board_history_cursor += 1;
        self.track_pending(operation.sequence);
        self.keep_focus_valid();
        Ok(())
    }

    pub(super) fn keep_focus_valid(&mut self) {
        self.insertion_index = self.insertion_index.min(self.board.live_thoughts().len());
        if self
            .focused_thought
            .is_some_and(|id| self.live_thought(id).is_ok())
        {
            return;
        }
        self.focused_thought = self.board.live_thoughts().first().map(|thought| thought.id);
        if matches!(self.mode, InteractionMode::Edit { .. }) {
            self.mode = InteractionMode::Board;
        }
    }
}

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
    /// Request exact-content copy through the clipboard port.
    CopyThought {
        /// Idempotent external request identity.
        request_id: RequestId,
        /// Thought to copy.
        thought_id: ThoughtId,
    },
    /// Request exact-content cut, deferring deletion until success.
    CutThought {
        /// Idempotent external request identity.
        request_id: RequestId,
        /// Durable operation used only after success.
        operation_id: OperationId,
        /// Thought to cut.
        thought_id: ThoughtId,
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
        presentation: crate::domain::ThoughtPresentation,
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
        presentation: crate::domain::ThoughtPresentation,
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
