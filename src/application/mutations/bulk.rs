//! Atomic board mutations over an explicit thought selection.

use super::{
    AppState, ApplicationError, ApplicationResult, BoardMutation, BoardOperation,
    BoardOperationKind, Effect, OperationId, Thought, ThoughtId, Timestamp,
};

pub(in crate::application) fn delete_thoughts(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids.len() == 1 {
        return super::delete_thought(state, operation_id, thought_ids[0], kind, at);
    }
    validate_deletion(thought_ids, kind)?;
    let selected = selected_thoughts(state, thought_ids)?;
    let first_index = usize::try_from(selected[0].position.get()).unwrap_or(usize::MAX);
    let focus_removed = state
        .focused_thought
        .is_some_and(|focus| thought_ids.contains(&focus));
    let forward = selected
        .iter()
        .rev()
        .map(|thought| deletion(thought, Some(at)))
        .collect();
    let inverse = selected
        .iter()
        .map(|thought| deletion(thought, None))
        .collect();
    let operation = batch_operation(state, operation_id, kind, forward, inverse, at)?;
    state.record_board_operation(&operation)?;
    if focus_removed {
        let live = state.board.live_thoughts();
        state.focused_thought = live
            .get(first_index)
            .or_else(|| first_index.checked_sub(1).and_then(|index| live.get(index)))
            .map(|thought| thought.id);
    }
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn set_collapsed_many(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    collapsed: bool,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids.len() == 1 {
        return super::set_collapsed(state, operation_id, thought_ids[0], collapsed, at);
    }
    let changed = selected_thoughts(state, thought_ids)?
        .into_iter()
        .filter(|thought| thought.collapsed != collapsed)
        .map(|thought| (thought.id, thought.collapsed))
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let forward = changed
        .iter()
        .map(|(id, _)| collapse(*id, collapsed))
        .collect();
    let inverse = changed
        .iter()
        .map(|(id, previous)| collapse(*id, *previous))
        .collect();
    let operation = batch_operation(
        state,
        operation_id,
        BoardOperationKind::Collapse,
        forward,
        inverse,
        at,
    )?;
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn validate_deletion(thought_ids: &[ThoughtId], kind: BoardOperationKind) -> ApplicationResult<()> {
    if thought_ids.is_empty()
        || !matches!(
            kind,
            BoardOperationKind::Delete
                | BoardOperationKind::Cut
                | BoardOperationKind::SubmitAndRemove
        )
    {
        return Err(ApplicationError::InvalidState);
    }
    Ok(())
}

fn selected_thoughts(
    state: &AppState,
    thought_ids: &[ThoughtId],
) -> ApplicationResult<Vec<Thought>> {
    let selected = state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| thought_ids.contains(&thought.id))
        .cloned()
        .collect::<Vec<_>>();
    if selected.len() != thought_ids.len() || selected.is_empty() {
        return Err(ApplicationError::InvalidState);
    }
    Ok(selected)
}

fn deletion(thought: &Thought, deleted_at: Option<Timestamp>) -> BoardMutation {
    BoardMutation::SetDeletion {
        thought_id: thought.id,
        deleted_at,
        position: thought.position,
    }
}

const fn collapse(thought_id: ThoughtId, collapsed: bool) -> BoardMutation {
    BoardMutation::SetCollapsed {
        thought_id,
        collapsed,
    }
}

fn batch_operation(
    state: &mut AppState,
    operation_id: OperationId,
    kind: BoardOperationKind,
    forward: Vec<BoardMutation>,
    inverse: Vec<BoardMutation>,
    at: Timestamp,
) -> ApplicationResult<BoardOperation> {
    Ok(BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence: state.next_sequence()?,
        kind,
        forward: BoardMutation::Batch { mutations: forward },
        inverse: BoardMutation::Batch { mutations: inverse },
        created_at: at,
    })
}
