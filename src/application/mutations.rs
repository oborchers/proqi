//! Focused domain mutations used by the reducer router.

use crate::{
    application::model::{
        AppState, ApplicationError, ApplicationResult, ClipboardIntent, Effect, FailureCode,
        InteractionMode, PendingClipboard,
    },
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, DomainError, OperationId, RequestId,
        RevisionId, TextPosition, Thought, ThoughtId, ThoughtPosition, ThoughtRevision, Timestamp,
        UndoScope,
    },
};

pub(super) fn create_thought(
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
pub(super) fn edit_thought(
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

pub(super) fn request_clipboard(
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

pub(super) fn finish_clipboard(
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

pub(super) fn delete_thought(
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

pub(super) fn move_thought(
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

pub(super) fn set_collapsed(
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

pub(super) fn history_move(
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
                .get(&thought_id)
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
            let (expected, content) = if undo {
                (&revision.after_content, revision.before_content)
            } else {
                (&revision.before_content, revision.after_content)
            };
            if &state.live_thought(thought_id)?.content != expected {
                return Err(ApplicationError::RevisionConflict(thought_id));
            }
            let thought = state
                .board
                .thought_mut(thought_id)
                .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
            thought.content = content;
            thought.updated_at = at;
            let history = state
                .editor_histories
                .get_mut(&thought_id)
                .ok_or(ApplicationError::InvalidState)?;
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
