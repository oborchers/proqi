//! Scriptable mixed Board-item mutations.

use crate::{
    cli::{args::ItemCommand, runtime::RuntimeContext},
    domain::BoardItemId,
};

use super::{
    CliError, Outcome, forwarding,
    helpers::{parse_item_id, parse_operation_id},
    item_mutation_outcome, session_service,
};

pub(super) fn execute(
    context: &mut RuntimeContext,
    command: ItemCommand,
) -> Result<Outcome, CliError> {
    match command {
        ItemCommand::InsertSeparator {
            session,
            position,
            operation_id,
        } => insert_separator(context, &session, position, operation_id.as_deref()),
        ItemCommand::Move {
            session,
            item,
            position,
            operation_id,
        } => move_item(context, &session, &item, position, operation_id.as_deref()),
        ItemCommand::Delete {
            session,
            items,
            operation_id,
        } => mutate_many(context, &session, &items, operation_id.as_deref(), false),
        ItemCommand::Duplicate {
            session,
            items,
            operation_id,
        } => mutate_many(context, &session, &items, operation_id.as_deref(), true),
    }
}

fn insert_separator(
    context: &mut RuntimeContext,
    session: &str,
    position: Option<usize>,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) = forwarding::insert_separator(context, session_id, position, operation)? {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let result = session_service(context)?.insert_separator(session_id, position, operation)?;
    Ok(item_mutation_outcome(&result.item_ids, result.receipt))
}

fn move_item(
    context: &mut RuntimeContext,
    session: &str,
    item: &str,
    position: usize,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let item_id = parse_item_id(item)?;
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) = forwarding::move_item(context, session_id, item_id, position, operation)?
    {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let result = session_service(context)?.move_item(session_id, item_id, position, operation)?;
    Ok(item_mutation_outcome(&result.item_ids, result.receipt))
}

fn mutate_many(
    context: &mut RuntimeContext,
    session: &str,
    items: &[String],
    operation: Option<&str>,
    duplicate: bool,
) -> Result<Outcome, CliError> {
    let item_ids = items
        .iter()
        .map(|value| parse_item_id(value))
        .collect::<Result<Vec<BoardItemId>, _>>()?;
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) =
        forwarding::mutate_items(context, session_id, item_ids.clone(), operation, duplicate)?
    {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let result = if duplicate {
        session_service(context)?.duplicate_items(session_id, item_ids, operation)?
    } else {
        session_service(context)?.delete_items(session_id, item_ids, operation)?
    };
    Ok(item_mutation_outcome(&result.item_ids, result.receipt))
}

fn resolve(
    context: &mut RuntimeContext,
    session: &str,
) -> Result<crate::domain::SessionId, CliError> {
    session_service(context)?
        .resolve_session(session, false)
        .map_err(Into::into)
}
