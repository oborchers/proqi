//! Exact thought transformations through the shared application service.

use crate::{cli::runtime::RuntimeContext, domain::ThoughtId};

use super::{
    CliError, Outcome,
    external_thoughts::parse_digest,
    forwarding,
    helpers::{parse_operation_id, parse_thought_id},
    item_mutation_outcome, session_service,
};

pub(super) fn split(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    at_byte: usize,
    expected: &str,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let expected = parse_digest(expected)?;
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) = forwarding::split_thought(
        context, session_id, thought_id, at_byte, expected, operation,
    )? {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let result = session_service(context)?
        .split_thought(session_id, thought_id, at_byte, expected, operation)?;
    Ok(item_mutation_outcome(&result.item_ids, result.receipt))
}

pub(super) fn extract(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    range: std::ops::Range<usize>,
    expected: &str,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let expected = parse_digest(expected)?;
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) = forwarding::extract_thought(
        context,
        session_id,
        thought_id,
        range.clone(),
        expected,
        operation,
    )? {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let result = session_service(context)?
        .extract_thought(session_id, thought_id, range, expected, operation)?;
    Ok(item_mutation_outcome(&result.item_ids, result.receipt))
}

pub(super) fn merge(
    context: &mut RuntimeContext,
    session: &str,
    thoughts: &[String],
    expected: &[String],
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    if thoughts.len() != expected.len() {
        return Err(CliError::arguments(
            "merge requires one --expected-sha256 value per thought".to_owned(),
        ));
    }
    let thought_ids = thoughts
        .iter()
        .map(|value| parse_thought_id(value))
        .collect::<Result<Vec<ThoughtId>, _>>()?;
    let expected = expected
        .iter()
        .map(|value| parse_digest(value))
        .collect::<Result<Vec<_>, _>>()?;
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) = forwarding::merge_thoughts(
        context,
        session_id,
        thought_ids.clone(),
        expected.clone(),
        operation,
    )? {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let separator = context.terminal_settings()?.ui.merge_separator;
    let result = session_service(context)?.merge_thoughts(
        session_id,
        thought_ids,
        &expected,
        separator,
        operation,
    )?;
    Ok(item_mutation_outcome(&result.item_ids, result.receipt))
}

pub(super) fn reflow(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    expected: &str,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let expected = parse_digest(expected)?;
    let operation = parse_operation_id(operation)?;
    let session_id = resolve(context, session)?;
    if let Some(result) =
        forwarding::reflow_thought(context, session_id, thought_id, expected, operation)?
    {
        return Ok(item_mutation_outcome(&result.item_ids, result.receipt));
    }
    let result =
        session_service(context)?.reflow_thought(session_id, thought_id, expected, operation)?;
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
