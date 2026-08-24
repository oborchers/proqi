//! Active-session CLI mutations forwarded to the verified reducer owner.

use serde_json::json;

use crate::{
    adapters::control::LocalControlClient,
    application::ThoughtMutation,
    domain::{ContentAnnotation, OperationId, SessionId, ThoughtId, UndoScope},
    ports::{
        control::{ControlClient, ControlError, ControlMutation, ControlRequest},
        environment::IdGenerator,
        runtime::{InstanceInfo, RuntimeCoordinator},
        store::CommitReceipt,
    },
};

use super::super::{output::CliError, runtime::RuntimeContext};

pub(super) fn add(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    add_annotated(context, session_id, body, Vec::new(), position, supplied)
}

pub(super) fn add_annotated(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    annotations: Vec<ContentAnnotation>,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|error| CliError::identifier(error.to_string()))?;
    let mutation = ControlMutation::Add {
        operation_id,
        thought_id,
        content: body.to_owned(),
        annotations,
        position,
    };
    let receipt = send(context, &owner, session_id, mutation)?;
    Ok(Some(ThoughtMutation {
        thought_id,
        receipt,
    }))
}

pub(super) fn delete(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let mutation = ControlMutation::Delete {
        operation_id,
        thought_id,
    };
    let receipt = send(context, &owner, session_id, mutation)?;
    Ok(Some(ThoughtMutation {
        thought_id,
        receipt,
    }))
}

pub(super) fn move_thought(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    position: usize,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let mutation = ControlMutation::Move {
        operation_id,
        thought_id,
        position,
    };
    let receipt = send(context, &owner, session_id, mutation)?;
    Ok(Some(ThoughtMutation {
        thought_id,
        receipt,
    }))
}

pub(super) fn history(
    context: &mut RuntimeContext,
    session_id: SessionId,
    scope: UndoScope,
    undo: bool,
    supplied: Option<OperationId>,
) -> Result<Option<CommitReceipt>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let mutation = ControlMutation::History {
        operation_id: supplied.unwrap_or_else(|| context.ids.operation_id()),
        scope,
        undo,
    };
    send(context, &owner, session_id, mutation).map(Some)
}

fn owner(
    context: &RuntimeContext,
    session_id: SessionId,
) -> Result<Option<InstanceInfo>, CliError> {
    Ok(context
        .coordinator
        .active_instances()?
        .into_iter()
        .find(|instance| instance.session_id == session_id))
}

fn send(
    context: &mut RuntimeContext,
    owner: &InstanceInfo,
    session_id: SessionId,
    mutation: ControlMutation,
) -> Result<CommitReceipt, CliError> {
    let protocol = owner.control_protocol.ok_or_else(|| {
        CliError::new(
            "session_busy",
            "active owner does not advertise a control protocol".to_owned(),
            5,
        )
    })?;
    if !(crate::ports::control::MIN_CONTROL_PROTOCOL_VERSION
        ..=crate::ports::control::CONTROL_PROTOCOL_VERSION)
        .contains(&protocol)
        || mutation.requires_protocol_two() && protocol < 2
    {
        return Err(CliError::new(
            "session_busy",
            "active owner does not support the required control protocol".to_owned(),
            5,
        ));
    }
    let request = ControlRequest {
        protocol,
        request_id: context.ids.request_id(),
        session_id,
        mutation,
    };
    LocalControlClient
        .send(owner, &request)
        .map(|receipt| receipt.durable)
        .map_err(|error| map_error(error, owner))
}

fn map_error(error: ControlError, owner: &InstanceInfo) -> CliError {
    let details = json!({
        "session_id": owner.session_id,
        "holder": owner,
    });
    match error {
        ControlError::Rejected { code, message } => match code.as_str() {
            "thought_not_found" => CliError::new("thought_not_found", message, 3),
            "storage_failed" => CliError::new("storage_failed", message, 1),
            "storage_busy" => CliError::new("storage_busy", message, 5),
            "storage_full" => CliError::new("storage_full", message, 1),
            "idempotency_conflict" => CliError::new("idempotency_conflict", message, 7),
            "no_durable_mutation" | "no_change" => CliError::new("no_change", message, 7),
            "outcome_unknown" => {
                CliError::new("operation_indeterminate", message, 8).with_details(details)
            }
            "owner_busy" | "protocol_mismatch" | "wrong_session" => {
                CliError::new("session_busy", message, 5).with_details(details)
            }
            _ => CliError::new("mutation_rejected", message, 7),
        },
        ControlError::MessageTooLarge => CliError::input(error.to_string()),
        ControlError::Unsupported
        | ControlError::InvalidPeer
        | ControlError::Protocol(_)
        | ControlError::Timeout
        | ControlError::Io(_) => {
            CliError::new("session_busy", error.to_string(), 5).with_details(details)
        }
    }
}
