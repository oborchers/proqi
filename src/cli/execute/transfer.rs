//! Durable cross-session thought delivery for the scriptable CLI.

use serde_json::json;

use crate::domain::{OperationId, SessionId, ThoughtId};
use crate::{
    application::{ControlReplay, match_control_replay},
    ports::{control::ControlMutation, store::Store},
};

use super::{CliError, Outcome, RuntimeContext, forwarding, parse_operation_id, parse_thought_id};

pub(super) fn send_thought(
    context: &mut RuntimeContext,
    source: &str,
    thought: &str,
    destination: &str,
    remove: bool,
    operation: Option<&str>,
    remove_operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let thought_id = parse_thought_id(thought)?;
    let operation = parse_operation_id(operation)?;
    let remove_operation = parse_operation_id(remove_operation)?;
    let (source_id, destination_id) = resolve_sessions(context, source, destination)?;
    if source_id == destination_id {
        return Err(CliError::arguments(
            "source and destination sessions must differ".to_owned(),
        ));
    }
    let thought = source_thought(
        context,
        source_id,
        thought_id,
        remove.then_some(remove_operation).flatten(),
    )?;
    let added = add_destination(context, destination_id, &thought, operation)?;
    let removed = if remove {
        match remove_source(context, source_id, thought_id, remove_operation) {
            Ok(receipt) => Some(receipt),
            Err(error) => {
                return Err(error.with_details(json!({
                    "destination_session_id": destination_id,
                    "destination_thought_id": added.thought_id,
                    "destination_receipt": added.receipt,
                    "source_removed": false,
                })));
            }
        }
    } else {
        None
    };
    Ok(transfer_outcome(source_id, destination_id, added, removed))
}

fn resolve_sessions(
    context: &mut RuntimeContext,
    source: &str,
    destination: &str,
) -> Result<(SessionId, SessionId), CliError> {
    let mut service = super::session_service(context)?;
    let source_id = service.resolve_session(source, false)?;
    let destination_id = service.resolve_session(destination, false)?;
    Ok((source_id, destination_id))
}

fn source_thought(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    replayed_removal: Option<OperationId>,
) -> Result<crate::domain::Thought, CliError> {
    let mut service = super::session_service(context)?;
    let snapshot = service.inspect_session(session_id)?;
    let thought = snapshot
        .board
        .thought(thought_id)
        .cloned()
        .ok_or_else(|| thought_not_found(thought_id))?;
    if thought.is_live() {
        return Ok(thought);
    }
    let Some(operation_id) = replayed_removal else {
        return Err(thought_not_found(thought_id));
    };
    verify_removal_replay(context, session_id, thought_id, operation_id)?;
    Ok(thought)
}

fn verify_removal_replay(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    operation_id: OperationId,
) -> Result<(), CliError> {
    let existing = context
        .store
        .operation_request(operation_id)?
        .ok_or_else(|| thought_not_found(thought_id))?;
    let mutation = ControlMutation::Delete {
        operation_id,
        thought_id,
    };
    match match_control_replay(&existing, session_id, &mutation) {
        ControlReplay::Accepted(_) => Ok(()),
        ControlReplay::Conflict => Err(CliError::new(
            "idempotency_conflict",
            "remove operation ID belongs to another mutation".to_owned(),
            7,
        )),
    }
}

fn thought_not_found(thought_id: ThoughtId) -> CliError {
    CliError::new(
        "thought_not_found",
        format!("thought not found: {thought_id}"),
        3,
    )
}

fn add_destination(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought: &crate::domain::Thought,
    operation: Option<OperationId>,
) -> Result<crate::application::ThoughtMutation, CliError> {
    if let Some(result) = forwarding::add_annotated(
        context,
        session_id,
        &thought.content,
        thought.annotations.clone(),
        None,
        operation,
    )? {
        return Ok(result);
    }
    let mut service = super::session_service(context)?;
    service
        .add_thought_annotated(
            session_id,
            thought.content.clone(),
            thought.annotations.clone(),
            None,
            operation,
        )
        .map_err(Into::into)
}

fn remove_source(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    operation: Option<OperationId>,
) -> Result<crate::ports::store::CommitReceipt, CliError> {
    if let Some(result) = forwarding::delete(context, session_id, thought_id, operation)? {
        return Ok(result.receipt);
    }
    let mut service = super::session_service(context)?;
    service
        .delete_thought(session_id, thought_id, operation)
        .map(|result| result.receipt)
        .map_err(Into::into)
}

fn transfer_outcome(
    source_id: SessionId,
    destination_id: SessionId,
    added: crate::application::ThoughtMutation,
    removed: Option<crate::ports::store::CommitReceipt>,
) -> Outcome {
    let source_removed = removed.is_some();
    Outcome {
        data: json!({
            "source_session_id": source_id,
            "source_removed": source_removed,
            "destination_session_id": destination_id,
            "destination_thought_id": added.thought_id,
            "destination_receipt": added.receipt,
            "source_removal_receipt": removed,
        }),
        human: format!(
            "Sent thought to {destination_id} as {}{}",
            added.thought_id,
            if source_removed {
                " and removed the source"
            } else {
                ""
            }
        ),
    }
}
