//! Payload-free separator creation and movement through Board history.

use crate::{
    application::{AppState, ApplicationError, ApplicationResult, Effect, InteractionMode},
    domain::{
        BoardItemId, BoardMutation, BoardOperation, BoardOperationKind, OperationId, Separator,
        SeparatorId, ThoughtPosition, Timestamp,
    },
};

use super::{move_thought, position_u32};
use crate::application::selected_move_steps;

pub(in crate::application) fn insert_separator(
    state: &mut AppState,
    separator_id: SeparatorId,
    operation_id: OperationId,
    insertion_index: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if insertion_index > state.board.live_items().len() {
        return Err(ApplicationError::InvalidState);
    }
    let position = ThoughtPosition::new(position_u32(insertion_index)?);
    let separator = Separator::new(separator_id, state.board.session.id, position, at);
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence: state.next_sequence()?,
        kind: BoardOperationKind::InsertSeparator,
        forward: BoardMutation::AddSeparator {
            separator: separator.clone(),
        },
        inverse: BoardMutation::SetSeparatorDeletion {
            separator_id,
            deleted_at: Some(at),
            position,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    state.focused_item = Some(BoardItemId::Separator(separator_id));
    state.mode = InteractionMode::Board;
    state.insertion_index = insertion_index.saturating_add(1);
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn move_item(
    state: &mut AppState,
    operation_id: OperationId,
    item_id: BoardItemId,
    to: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if to >= state.board.live_items().len() {
        return Err(ApplicationError::InvalidState);
    }
    if let BoardItemId::Thought(thought_id) = item_id {
        return move_thought(state, operation_id, thought_id, to, at);
    }
    let BoardItemId::Separator(separator_id) = item_id else {
        return Err(ApplicationError::InvalidState);
    };
    let from = state
        .board
        .item_position(item_id)
        .ok_or(ApplicationError::InvalidState)?;
    let to = ThoughtPosition::new(position_u32(to)?);
    if from == to {
        return Ok(Vec::new());
    }
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence: state.next_sequence()?,
        kind: BoardOperationKind::Reorder,
        forward: BoardMutation::MoveSeparator {
            separator_id,
            from,
            to,
        },
        inverse: BoardMutation::MoveSeparator {
            separator_id,
            from: to,
            to: from,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn move_items(
    state: &mut AppState,
    operation_id: OperationId,
    item_ids: &[BoardItemId],
    delta: isize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let order = state
        .board
        .live_items()
        .into_iter()
        .map(crate::domain::BoardItemRef::id)
        .collect::<Vec<_>>();
    let steps = selected_move_steps(&order, item_ids, delta)?;
    if steps.is_empty() {
        return Ok(Vec::new());
    }
    if steps.iter().any(|step| {
        step.item_id
            .thought()
            .is_some_and(|id| state.thought_locked(id))
    }) {
        return Err(ApplicationError::InvalidState);
    }
    let forward = steps
        .iter()
        .map(|step| item_move(step.item_id, step.from, step.to))
        .collect::<ApplicationResult<Vec<_>>>()?;
    let inverse = steps
        .iter()
        .rev()
        .map(|step| item_move(step.item_id, step.to, step.from))
        .collect::<ApplicationResult<Vec<_>>>()?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence: state.next_sequence()?,
        kind: BoardOperationKind::Reorder,
        forward: BoardMutation::Batch { mutations: forward },
        inverse: BoardMutation::Batch { mutations: inverse },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn item_move(item_id: BoardItemId, from: usize, to: usize) -> ApplicationResult<BoardMutation> {
    let from = ThoughtPosition::new(position_u32(from)?);
    let to = ThoughtPosition::new(position_u32(to)?);
    Ok(match item_id {
        BoardItemId::Thought(thought_id) => BoardMutation::MoveThought {
            thought_id,
            from,
            to,
        },
        BoardItemId::Separator(separator_id) => BoardMutation::MoveSeparator {
            separator_id,
            from,
            to,
        },
    })
}
