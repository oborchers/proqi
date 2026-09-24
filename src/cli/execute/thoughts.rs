//! Thought inspection and mutation commands for one explicit session.

use crate::domain::UndoScope;

use super::{
    super::args::{HistoryArgs, ThoughtCommand},
    CliError, Outcome, RuntimeContext, external_thoughts, forwarding,
    helpers::{parse_operation_id, parse_thought_id, read_standard_input},
    mutation_outcome, queries, receipt_outcome, session_service, thought_names, transfer,
    transformations,
};

#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive typed command dispatcher keeps each route visible"
)]
pub(super) fn execute(
    context: &mut RuntimeContext,
    command: ThoughtCommand,
) -> Result<Outcome, CliError> {
    match command {
        ThoughtCommand::List { session, page } => queries::list(context, &session, &page),
        ThoughtCommand::Inspect { session, thought } => {
            queries::inspect(context, &session, &thought)
        }
        ThoughtCommand::Add {
            session,
            name,
            position,
            operation_id,
        } => add_thought(
            context,
            &session,
            name.as_deref(),
            position,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Delete {
            session,
            thought,
            operation_id,
        } => delete_thought(context, &session, &thought, operation_id.as_deref()),
        ThoughtCommand::Rename {
            session,
            thought,
            name,
            clear: _,
            operation_id,
        } => thought_names::rename_thought(
            context,
            &session,
            &thought,
            name.as_deref(),
            operation_id.as_deref(),
        ),
        ThoughtCommand::Replace {
            session,
            thought,
            revision_id,
            expected_sha256,
            force,
        } => external_thoughts::replace(
            context,
            &session,
            &thought,
            revision_id.as_deref(),
            expected_sha256.as_deref(),
            force,
        ),
        ThoughtCommand::Collapse {
            session,
            thought,
            collapsed,
            operation_id,
        } => external_thoughts::collapse(
            context,
            &session,
            &thought,
            collapsed,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Move {
            session,
            thought,
            position,
            operation_id,
        } => move_thought(
            context,
            &session,
            &thought,
            position,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Split {
            session,
            thought,
            at_byte,
            expected_sha256,
            operation_id,
        } => transformations::split(
            context,
            &session,
            &thought,
            at_byte,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Extract {
            session,
            thought,
            start_byte,
            end_byte,
            expected_sha256,
            operation_id,
        } => transformations::extract(
            context,
            &session,
            &thought,
            start_byte..end_byte,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Merge {
            session,
            thoughts,
            expected_sha256,
            operation_id,
        } => transformations::merge(
            context,
            &session,
            &thoughts,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Reflow {
            session,
            thought,
            expected_sha256,
            operation_id,
        } => transformations::reflow(
            context,
            &session,
            &thought,
            &expected_sha256,
            operation_id.as_deref(),
        ),
        ThoughtCommand::Send {
            source,
            thought,
            destination,
            remove,
            operation_id,
            remove_operation_id,
        } => execute_send_thought(
            context,
            &source,
            &thought,
            &destination,
            remove,
            operation_id.as_deref(),
            remove_operation_id.as_deref(),
        ),
        ThoughtCommand::Undo(arguments) => move_history(context, &arguments, true),
        ThoughtCommand::Redo(arguments) => move_history(context, &arguments, false),
    }
}

fn execute_send_thought(
    context: &mut RuntimeContext,
    source: &str,
    thought: &str,
    destination: &str,
    remove: bool,
    operation_id: Option<&str>,
    remove_operation_id: Option<&str>,
) -> Result<Outcome, CliError> {
    transfer::send_thought(
        context,
        source,
        thought,
        destination,
        remove,
        operation_id,
        remove_operation_id,
    )
}

fn add_thought(
    context: &mut RuntimeContext,
    session: &str,
    name: Option<&str>,
    position: Option<usize>,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let name = name.map(thought_names::parse_thought_name).transpose()?;
    let operation = parse_operation_id(operation)?;
    let body = read_standard_input()?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    let Some(name) = name else {
        if let Some(result) = forwarding::add(context, session_id, &body, position, operation)? {
            return Ok(mutation_outcome(result.thought_id, result.receipt));
        }
        let result =
            session_service(context)?.add_thought(session_id, body, position, operation)?;
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    };
    let forwarded = forwarding::preserve_add(
        context,
        session_id,
        &body,
        Vec::new(),
        Some(name.clone()),
        position,
        operation,
    )?;
    if let Some(result) = forwarded {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let result = session_service(context)?.preserve_thought(
        session_id,
        body,
        Vec::new(),
        Some(name),
        position,
        operation,
    )?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn delete_thought(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let operation = parse_operation_id(operation)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) = forwarding::delete(context, session_id, thought_id, operation)? {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let mut service = session_service(context)?;
    let result = service.delete_thought(session_id, thought_id, operation)?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn move_thought(
    context: &mut RuntimeContext,
    session: &str,
    thought: &str,
    position: usize,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let operation = parse_operation_id(operation)?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(session, false)?;
    drop(service);
    if let Some(result) =
        forwarding::move_thought(context, session_id, thought_id, position, operation)?
    {
        return Ok(mutation_outcome(result.thought_id, result.receipt));
    }
    let mut service = session_service(context)?;
    let result = service.move_thought(session_id, thought_id, position, operation)?;
    Ok(mutation_outcome(result.thought_id, result.receipt))
}

fn move_history(
    context: &mut RuntimeContext,
    arguments: &HistoryArgs,
    undo: bool,
) -> Result<Outcome, CliError> {
    let thought = arguments
        .thought
        .as_deref()
        .map(parse_thought_id)
        .transpose()?;
    let scope = thought.map_or(UndoScope::Board, |thought_id| UndoScope::Editor {
        thought_id,
    });
    let operation = parse_operation_id(arguments.operation_id.as_deref())?;
    let mut service = session_service(context)?;
    let session_id = service.resolve_session(&arguments.session, false)?;
    drop(service);
    if let Some(receipt) = forwarding::history(context, session_id, scope, undo, operation)? {
        return Ok(receipt_outcome(receipt));
    }
    let mut service = session_service(context)?;
    let receipt = service.move_history(session_id, scope, undo, operation)?;
    Ok(receipt_outcome(receipt))
}
