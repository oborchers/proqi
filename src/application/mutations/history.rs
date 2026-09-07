//! Exact board and editor history replay without allocating attachment identities.

use super::{
    AppState, ApplicationError, ApplicationResult, BoardMutation, BoardOperation,
    BoardOperationKind, Effect, OperationId, ThoughtId, Timestamp,
};
use crate::domain::{Thought, UndoScope};

pub(in crate::application) fn history_move(
    state: &mut AppState,
    operation_id: OperationId,
    scope: UndoScope,
    at: Timestamp,
    undo: bool,
) -> ApplicationResult<Vec<Effect>> {
    let sequence = state.next_sequence()?;
    match scope {
        UndoScope::Board => move_board_history(state, at, undo)?,
        UndoScope::Editor { thought_id } => move_editor_history(state, thought_id, at, undo)?,
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

fn move_board_history(state: &mut AppState, at: Timestamp, undo: bool) -> ApplicationResult<()> {
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
    let focused_before = state.focused_thought;
    let transform_source = undo.then(|| transform_source(&operation)).flatten();
    let mut board = state.board.clone();
    board.apply_mutation(mutation, at)?;
    state.board = board;
    state.board_history_cursor =
        state
            .board_history_cursor
            .saturating_add_signed(if undo { -1 } else { 1 });
    let focus_was_removed = focused_before.is_some_and(|thought_id| {
        state
            .board
            .thought(thought_id)
            .is_none_or(|thought| !thought.is_live())
    });
    state.keep_focus_valid();
    if focus_was_removed
        && let Some(thought_id) = transform_source
        && state
            .board
            .thought(thought_id)
            .is_some_and(Thought::is_live)
    {
        state.focused_thought = Some(thought_id);
    }
    Ok(())
}

fn transform_source(operation: &BoardOperation) -> Option<ThoughtId> {
    if !matches!(
        operation.kind,
        BoardOperationKind::Split | BoardOperationKind::Extract
    ) {
        return None;
    }
    replaced_thought(&operation.forward)
}

fn replaced_thought(mutation: &BoardMutation) -> Option<ThoughtId> {
    match mutation {
        BoardMutation::Batch { mutations } => mutations.iter().find_map(replaced_thought),
        BoardMutation::ReplaceContent { thought_id, .. } => Some(*thought_id),
        BoardMutation::AddThought { .. }
        | BoardMutation::SetDeletion { .. }
        | BoardMutation::SetDeletionExact { .. }
        | BoardMutation::MoveThought { .. }
        | BoardMutation::SetPresentation { .. }
        | BoardMutation::LegacySetCollapsed { .. } => None,
    }
}

fn move_editor_history(
    state: &mut AppState,
    thought_id: ThoughtId,
    at: Timestamp,
    undo: bool,
) -> ApplicationResult<()> {
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
    let (expected, expected_annotations, content, annotations) = if undo {
        (
            &revision.after_content,
            &revision.after_annotations,
            revision.before_content,
            revision.before_annotations,
        )
    } else {
        (
            &revision.before_content,
            &revision.before_annotations,
            revision.after_content,
            revision.after_annotations,
        )
    };
    let current = state.live_thought(thought_id)?;
    if &current.content != expected || &current.annotations != expected_annotations {
        return Err(ApplicationError::RevisionConflict(thought_id));
    }
    let thought = state
        .board
        .thought_mut(thought_id)
        .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
    thought.content = content;
    thought.annotations = annotations;
    thought.updated_at = at;
    let history = state
        .editor_histories
        .get_mut(&thought_id)
        .ok_or(ApplicationError::InvalidState)?;
    history.cursor = history
        .cursor
        .saturating_add_signed(if undo { -1 } else { 1 });
    Ok(())
}
