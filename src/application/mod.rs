//! Application state, normalized actions, effects, errors, and reducer.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, DomainError, OperationId,
        OperationSequence, RequestId, RevisionId, SessionBoard, SessionId, Thought, ThoughtId,
        ThoughtPosition, ThoughtRevision, Timestamp, UndoScope,
    },
    ports::editor::TextPosition,
};

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

/// Stable application error and notification codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FailureCode {
    /// Requested thought is absent or deleted.
    ThoughtNotFound,
    /// Action is not valid in the current mode or history state.
    InvalidState,
    /// Clipboard access failed without mutating content.
    ClipboardFailed,
    /// Persistence failed and state is not yet durable.
    StorageFailed,
    /// Domain invariants rejected the operation.
    InvariantViolation,
}

impl FailureCode {
    /// Stable machine-readable spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThoughtNotFound => "thought_not_found",
            Self::InvalidState => "invalid_state",
            Self::ClipboardFailed => "clipboard_failed",
            Self::StorageFailed => "storage_failed",
            Self::InvariantViolation => "invariant_violation",
        }
    }
}

/// Typed application failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ApplicationError {
    /// Domain validation failed.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Referenced thought is unavailable.
    #[error("live thought not found: {0}")]
    ThoughtNotFound(ThoughtId),
    /// Revision does not match current content or ownership.
    #[error("revision precondition failed for thought {0}")]
    RevisionConflict(ThoughtId),
    /// An action requires another interaction state.
    #[error("action is invalid in the current application state")]
    InvalidState,
    /// Operation sequence cannot increase.
    #[error("operation sequence exhausted")]
    SequenceExhausted,
}

impl ApplicationError {
    /// Stable machine-readable classification.
    #[must_use]
    pub const fn code(&self) -> FailureCode {
        match self {
            Self::ThoughtNotFound(_) => FailureCode::ThoughtNotFound,
            Self::RevisionConflict(_) | Self::InvalidState | Self::SequenceExhausted => {
                FailureCode::InvalidState
            }
            Self::Domain(_) => FailureCode::InvariantViolation,
        }
    }
}

/// Application result type.
pub type ApplicationResult<T> = Result<T, ApplicationError>;

/// Clipboard purpose, kept separate from terminal keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardIntent {
    /// Preserve the thought after writing exact content.
    Copy,
    /// Delete only after a successful write.
    Cut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingClipboard {
    thought_id: ThoughtId,
    intent: ClipboardIntent,
    operation_id: Option<OperationId>,
    at: Timestamp,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct EditorHistory {
    revisions: Vec<ThoughtRevision>,
    cursor: usize,
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
    board_history: Vec<BoardOperation>,
    board_history_cursor: usize,
    editor_histories: HashMap<ThoughtId, EditorHistory>,
    pending_clipboard: BTreeMap<RequestId, PendingClipboard>,
    pending_sequences: BTreeSet<OperationSequence>,
    highest_sequence: OperationSequence,
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

    fn next_sequence(&self) -> ApplicationResult<OperationSequence> {
        self.highest_sequence
            .checked_next()
            .ok_or(ApplicationError::SequenceExhausted)
    }

    fn track_pending(&mut self, sequence: OperationSequence) {
        self.highest_sequence = sequence;
        self.pending_sequences.insert(sequence);
        self.refresh_durability();
    }

    fn refresh_durability(&mut self) {
        let durable = self.board.session.last_durable_sequence;
        self.durability = self.pending_sequences.last().map_or(
            DurabilityState::Durable { sequence: durable },
            |latest| DurabilityState::Pending {
                durable,
                latest: *latest,
            },
        );
    }

    fn live_thought(&self, id: ThoughtId) -> ApplicationResult<&Thought> {
        self.board
            .thought(id)
            .filter(|thought| thought.is_live())
            .ok_or(ApplicationError::ThoughtNotFound(id))
    }

    fn record_board_operation(&mut self, operation: &BoardOperation) -> ApplicationResult<()> {
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

    fn keep_focus_valid(&mut self) {
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
        self.insertion_index = self.insertion_index.min(self.board.live_thoughts().len());
    }
}

/// Normalized input or external result accepted by the reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
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
    /// Set the explicit collapse preference.
    SetCollapsed {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Affected thought.
        thought_id: ThoughtId,
        /// New preference.
        collapsed: bool,
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
}

/// Blocking work requested by the pure reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Commit one new structural operation.
    CommitBoardOperation(BoardOperation),
    /// Commit one new editor revision.
    CommitRevision(ThoughtRevision),
    /// Atomically move one persistent history cursor and current state.
    CommitHistoryMove {
        /// Idempotent durable control identity.
        operation_id: OperationId,
        /// Owning session.
        session_id: SessionId,
        /// Explicit history scope.
        scope: UndoScope,
        /// Undo when true, redo when false.
        undo: bool,
        /// New monotonic commit sequence.
        sequence: OperationSequence,
        /// Event time.
        at: Timestamp,
    },
    /// Write exact content through the clipboard adapter.
    WriteClipboard {
        /// Matching request identity.
        request_id: RequestId,
        /// Source thought.
        thought_id: ThoughtId,
        /// Copy or cut intent.
        intent: ClipboardIntent,
        /// Exact content, including line endings.
        content: String,
    },
    /// Present a non-destructive user-visible status.
    Notify {
        /// Stable classification.
        code: FailureCode,
    },
}

/// Reduce one action into current state and ordered effects.
///
/// # Errors
///
/// Returns a typed error when an action violates current state or domain invariants.
#[allow(clippy::too_many_lines)]
pub fn reduce(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::FocusThought(focus) => {
            if let Some(id) = focus {
                state.live_thought(id)?;
            }
            state.focused_thought = focus;
            Ok(Vec::new())
        }
        Action::EnterEdit(thought_id) => {
            state.live_thought(thought_id)?;
            state.focused_thought = Some(thought_id);
            state.mode = InteractionMode::Edit { thought_id };
            Ok(Vec::new())
        }
        Action::ExitEdit => {
            state.mode = InteractionMode::Board;
            Ok(Vec::new())
        }
        Action::CreateThought {
            thought_id,
            operation_id,
            content,
            insertion_index,
            at,
        } => create_thought(
            state,
            thought_id,
            operation_id,
            content,
            insertion_index.unwrap_or(state.insertion_index),
            at,
        ),
        Action::PasteAsThought {
            thought_id,
            operation_id,
            content,
            at,
        } => create_thought(
            state,
            thought_id,
            operation_id,
            content,
            state.insertion_index,
            at,
        ),
        Action::EditThought {
            thought_id,
            revision_id,
            before_content,
            after_content,
            before_cursor,
            after_cursor,
            at,
        } => edit_thought(
            state,
            thought_id,
            revision_id,
            before_content,
            after_content,
            before_cursor,
            after_cursor,
            at,
        ),
        Action::CopyThought {
            request_id,
            thought_id,
        } => request_clipboard(
            state,
            request_id,
            thought_id,
            ClipboardIntent::Copy,
            None,
            None,
        ),
        Action::CutThought {
            request_id,
            operation_id,
            thought_id,
            at,
        } => request_clipboard(
            state,
            request_id,
            thought_id,
            ClipboardIntent::Cut,
            Some(operation_id),
            Some(at),
        ),
        Action::ClipboardResult { request_id, result } => {
            finish_clipboard(state, request_id, result)
        }
        Action::DeleteThought {
            operation_id,
            thought_id,
            kind,
            at,
        } => delete_thought(state, operation_id, thought_id, kind, at),
        Action::MoveThought {
            operation_id,
            thought_id,
            to,
            at,
        } => move_thought(state, operation_id, thought_id, to, at),
        Action::SetCollapsed {
            operation_id,
            thought_id,
            collapsed,
            at,
        } => set_collapsed(state, operation_id, thought_id, collapsed, at),
        Action::Undo {
            operation_id,
            scope,
            at,
        } => history_move(state, operation_id, scope, at, true),
        Action::Redo {
            operation_id,
            scope,
            at,
        } => history_move(state, operation_id, scope, at, false),
        Action::PersistenceCommitted(sequence) => {
            if state.pending_sequences.first().copied() != Some(sequence) {
                return Err(ApplicationError::InvalidState);
            }
            if state.pending_sequences.remove(&sequence) {
                state.board.session.last_durable_sequence =
                    state.board.session.last_durable_sequence.max(sequence);
            }
            state.refresh_durability();
            Ok(Vec::new())
        }
        Action::PersistenceFailed { sequence, code } => {
            if !state.pending_sequences.contains(&sequence) {
                return Err(ApplicationError::InvalidState);
            }
            state.durability = DurabilityState::Failed {
                durable: state.board.session.last_durable_sequence,
                failed: sequence,
                code,
            };
            Ok(vec![Effect::Notify { code }])
        }
    }
}

fn create_thought(
    state: &mut AppState,
    thought_id: ThoughtId,
    operation_id: OperationId,
    content: String,
    insertion_index: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let sequence = state.next_sequence()?;
    let thought = Thought::new(
        thought_id,
        state.board.session.id,
        content,
        ThoughtPosition::new(position_u32(insertion_index)?),
        at,
    );
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind: BoardOperationKind::Create,
        forward: BoardMutation::AddThought {
            thought: thought.clone(),
        },
        inverse: BoardMutation::SetDeletion {
            thought_id,
            deleted_at: Some(at),
            position: thought.position,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    state.focused_thought = Some(thought_id);
    state.mode = InteractionMode::Edit { thought_id };
    state.insertion_index = insertion_index + 1;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

#[allow(clippy::too_many_arguments)]
fn edit_thought(
    state: &mut AppState,
    thought_id: ThoughtId,
    revision_id: RevisionId,
    before_content: String,
    after_content: String,
    before_cursor: TextPosition,
    after_cursor: TextPosition,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let current = state.live_thought(thought_id)?;
    if current.content != before_content {
        return Err(ApplicationError::RevisionConflict(thought_id));
    }
    if before_content == after_content {
        return Ok(Vec::new());
    }
    let sequence = state.next_sequence()?;
    let revision = ThoughtRevision {
        id: revision_id,
        session_id: state.board.session.id,
        thought_id,
        sequence,
        before_content,
        after_content: after_content.clone(),
        before_cursor,
        after_cursor,
        created_at: at,
    };
    let mut board = state.board.clone();
    let thought = board
        .thought_mut(thought_id)
        .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
    thought.content = after_content;
    thought.updated_at = at;
    state.board = board;
    let history = state.editor_histories.entry(thought_id).or_default();
    history.revisions.truncate(history.cursor);
    history.revisions.push(revision.clone());
    history.cursor += 1;
    state.track_pending(sequence);
    Ok(vec![Effect::CommitRevision(revision)])
}

fn request_clipboard(
    state: &mut AppState,
    request_id: RequestId,
    thought_id: ThoughtId,
    intent: ClipboardIntent,
    operation_id: Option<OperationId>,
    at: Option<Timestamp>,
) -> ApplicationResult<Vec<Effect>> {
    let content = state.live_thought(thought_id)?.content.clone();
    state
        .pending_clipboard
        .entry(request_id)
        .or_insert(PendingClipboard {
            thought_id,
            intent,
            operation_id,
            at: at.unwrap_or_default(),
        });
    Ok(vec![Effect::WriteClipboard {
        request_id,
        thought_id,
        intent,
        content,
    }])
}

fn finish_clipboard(
    state: &mut AppState,
    request_id: RequestId,
    result: Result<(), FailureCode>,
) -> ApplicationResult<Vec<Effect>> {
    let Some(pending) = state.pending_clipboard.remove(&request_id) else {
        return Ok(Vec::new());
    };
    if let Err(code) = result {
        return Ok(vec![Effect::Notify { code }]);
    }
    if pending.intent == ClipboardIntent::Cut {
        return delete_thought(
            state,
            pending.operation_id.ok_or(ApplicationError::InvalidState)?,
            pending.thought_id,
            BoardOperationKind::Cut,
            pending.at,
        );
    }
    Ok(Vec::new())
}

fn delete_thought(
    state: &mut AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if !matches!(
        kind,
        BoardOperationKind::Delete | BoardOperationKind::Cut | BoardOperationKind::SubmitAndRemove
    ) {
        return Err(ApplicationError::InvalidState);
    }
    let thought = state.live_thought(thought_id)?.clone();
    let sequence = state.next_sequence()?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind,
        forward: BoardMutation::SetDeletion {
            thought_id,
            deleted_at: Some(at),
            position: thought.position,
        },
        inverse: BoardMutation::SetDeletion {
            thought_id,
            deleted_at: None,
            position: thought.position,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn move_thought(
    state: &mut AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    to: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let thought = state.live_thought(thought_id)?;
    let from = thought.position;
    let to = ThoughtPosition::new(position_u32(to)?);
    if from == to {
        return Ok(Vec::new());
    }
    let sequence = state.next_sequence()?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind: BoardOperationKind::Reorder,
        forward: BoardMutation::MoveThought {
            thought_id,
            from,
            to,
        },
        inverse: BoardMutation::MoveThought {
            thought_id,
            from: to,
            to: from,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn set_collapsed(
    state: &mut AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    collapsed: bool,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let previous = state.live_thought(thought_id)?.collapsed;
    if previous == collapsed {
        return Ok(Vec::new());
    }
    let sequence = state.next_sequence()?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind: BoardOperationKind::Collapse,
        forward: BoardMutation::SetCollapsed {
            thought_id,
            collapsed,
        },
        inverse: BoardMutation::SetCollapsed {
            thought_id,
            collapsed: previous,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn history_move(
    state: &mut AppState,
    operation_id: OperationId,
    scope: UndoScope,
    at: Timestamp,
    undo: bool,
) -> ApplicationResult<Vec<Effect>> {
    let sequence = state.next_sequence()?;
    match scope {
        UndoScope::Board => {
            let operation = if undo {
                state
                    .board_history_cursor
                    .checked_sub(1)
                    .and_then(|index| state.board_history.get(index))
            } else {
                state.board_history.get(state.board_history_cursor)
            }
            .cloned()
            .ok_or(ApplicationError::InvalidState)?;
            let mutation = if undo {
                &operation.inverse
            } else {
                &operation.forward
            };
            let mut board = state.board.clone();
            board.apply_mutation(mutation, at)?;
            state.board = board;
            if undo {
                state.board_history_cursor -= 1;
            } else {
                state.board_history_cursor += 1;
            }
            state.keep_focus_valid();
        }
        UndoScope::Editor { thought_id } => {
            let history = state
                .editor_histories
                .get_mut(&thought_id)
                .ok_or(ApplicationError::InvalidState)?;
            let revision = if undo {
                history
                    .cursor
                    .checked_sub(1)
                    .and_then(|index| history.revisions.get(index))
            } else {
                history.revisions.get(history.cursor)
            }
            .cloned()
            .ok_or(ApplicationError::InvalidState)?;
            let content = if undo {
                revision.before_content
            } else {
                revision.after_content
            };
            let thought = state
                .board
                .thought_mut(thought_id)
                .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
            thought.content = content;
            thought.updated_at = at;
            if undo {
                history.cursor -= 1;
            } else {
                history.cursor += 1;
            }
        }
    }
    state.track_pending(sequence);
    Ok(vec![Effect::CommitHistoryMove {
        operation_id,
        session_id: state.board.session.id,
        scope,
        undo,
        sequence,
        at,
    }])
}

fn position_u32(value: usize) -> Result<u32, ApplicationError> {
    u32::try_from(value).map_err(|_| {
        ApplicationError::Domain(DomainError::InvalidPosition {
            requested: value,
            len: usize::try_from(u32::MAX).unwrap_or(usize::MAX),
        })
    })
}
