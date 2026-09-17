//! Payload-free separator creation and movement through Board history.

use crate::{
    application::{AppState, ApplicationError, ApplicationResult, Effect, InteractionMode},
    domain::{
        BoardItemId, BoardMutation, BoardOperation, BoardOperationKind, OperationId, Separator,
        SeparatorId, ThoughtPosition, Timestamp,
    },
};

use super::{move_thought, position_u32};

pub(in crate::application) fn insert_separator(
    state: &mut AppState,
    separator_id: SeparatorId,
    operation_id: OperationId,
    insertion_index: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
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
