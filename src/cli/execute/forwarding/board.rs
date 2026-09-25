//! Active-owner forwarding for mixed Board and text-transformation mutations.

use crate::{
    application::BoardItemMutation,
    domain::{BoardItemId, OperationId, SessionId, ThoughtId},
    ports::{control::ControlMutation, environment::IdGenerator},
};

use super::{owner, send_control};
use crate::cli::{output::CliError, runtime::RuntimeContext};

pub(in crate::cli::execute) fn insert_separator(
    context: &mut RuntimeContext,
    session_id: SessionId,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<BoardItemMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let separator_id =
        crate::domain::SeparatorId::from_database_bytes(operation_id.database_bytes())
            .map_err(|error| CliError::identifier(error.to_string()))?;
    let receipt = send_control(
        context,
        &owner,
        session_id,
        ControlMutation::InsertSeparator {
            operation_id,
            separator_id,
            position,
        },
    )?;
    Ok(Some(BoardItemMutation {
        item_ids: receipt.item_ids,
        receipt: receipt.durable,
    }))
}

pub(in crate::cli::execute) fn move_item(
    context: &mut RuntimeContext,
    session_id: SessionId,
    item_id: BoardItemId,
    position: usize,
    supplied: Option<OperationId>,
) -> Result<Option<BoardItemMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let receipt = send_control(
        context,
        &owner,
        session_id,
        ControlMutation::MoveItem {
            operation_id,
            item_id,
            position,
        },
    )?;
    Ok(Some(BoardItemMutation {
        item_ids: receipt.item_ids,
        receipt: receipt.durable,
    }))
}

pub(in crate::cli::execute) fn mutate_items(
    context: &mut RuntimeContext,
    session_id: SessionId,
    item_ids: Vec<BoardItemId>,
    supplied: Option<OperationId>,
    duplicate: bool,
) -> Result<Option<BoardItemMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let mutation = if duplicate {
        let duplicate_ids =
            crate::application::derived_duplicate_item_ids(operation_id, &item_ids)?;
        ControlMutation::DuplicateItems {
            operation_id,
            item_ids,
            duplicate_ids,
        }
    } else {
        ControlMutation::DeleteItems {
            operation_id,
            item_ids,
        }
    };
    let receipt = send_control(context, &owner, session_id, mutation)?;
    Ok(Some(BoardItemMutation {
        item_ids: receipt.item_ids,
        receipt: receipt.durable,
    }))
}

pub(in crate::cli::execute) fn split_thought(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    at_byte: usize,
    expected_digest: [u8; 32],
    supplied: Option<OperationId>,
) -> Result<Option<BoardItemMutation>, CliError> {
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let new_thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|error| CliError::identifier(error.to_string()))?;
    forward_items(
        context,
        session_id,
        ControlMutation::SplitThought {
            operation_id,
            thought_id,
            new_thought_id,
            expected_digest,
            at_byte,
        },
    )
}

pub(in crate::cli::execute) fn extract_thought(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    range: std::ops::Range<usize>,
    expected_digest: [u8; 32],
    supplied: Option<OperationId>,
) -> Result<Option<BoardItemMutation>, CliError> {
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let new_thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|error| CliError::identifier(error.to_string()))?;
    forward_items(
        context,
        session_id,
        ControlMutation::ExtractThought {
            operation_id,
            thought_id,
            new_thought_id,
            expected_digest,
            start_byte: range.start,
            end_byte: range.end,
        },
    )
}

pub(in crate::cli::execute) fn merge_thoughts(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_ids: Vec<ThoughtId>,
    expected_digests: Vec<[u8; 32]>,
    supplied: Option<OperationId>,
) -> Result<Option<BoardItemMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    // The live owner replaces this transport placeholder with its validated
    // launch-time setting before replay matching or reducer execution.
    let receipt = send_control(
        context,
        &owner,
        session_id,
        ControlMutation::MergeThoughts {
            operation_id,
            thought_ids,
            expected_digests,
            separator: String::new(),
        },
    )?;
    Ok(Some(BoardItemMutation {
        item_ids: receipt.item_ids,
        receipt: receipt.durable,
    }))
}

pub(in crate::cli::execute) fn reflow_thought(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    expected_digest: [u8; 32],
    supplied: Option<OperationId>,
) -> Result<Option<BoardItemMutation>, CliError> {
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    forward_items(
        context,
        session_id,
        ControlMutation::ReflowThought {
            operation_id,
            thought_id,
            expected_digest,
        },
    )
}

pub(in crate::cli::execute) fn export_thoughts(
    context: &mut RuntimeContext,
    session_id: SessionId,
    request: ControlMutation,
) -> Result<Option<BoardItemMutation>, CliError> {
    forward_items(context, session_id, request)
}

fn forward_items(
    context: &mut RuntimeContext,
    session_id: SessionId,
    mutation: ControlMutation,
) -> Result<Option<BoardItemMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let receipt = send_control(context, &owner, session_id, mutation)?;
    Ok(Some(BoardItemMutation {
        item_ids: receipt.item_ids,
        receipt: receipt.durable,
    }))
}
