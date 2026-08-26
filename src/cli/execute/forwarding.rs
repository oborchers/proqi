//! Active-session CLI mutations forwarded to the verified reducer owner.

use serde_json::json;

use crate::{
    adapters::control::LocalControlClient,
    application::ThoughtMutation,
    domain::{ContentAnnotation, OperationId, RevisionId, SessionId, ThoughtId, UndoScope},
    ports::{
        control::{ControlClient, ControlError, ControlMutation, ControlRequest},
        environment::IdGenerator,
        runtime::{InstanceInfo, RuntimeCoordinator},
        store::CommitReceipt,
    },
};

use super::super::{output::CliError, runtime::RuntimeContext};

pub(super) fn rename_session(
    context: &mut RuntimeContext,
    session_id: SessionId,
    name: Option<String>,
) -> Result<bool, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(false);
    };
    let mutation = ControlMutation::RenameSession { name };
    let protocol = required_protocol(&owner, &mutation)?;
    let request = ControlRequest {
        protocol,
        request_id: context.ids.request_id(),
        session_id,
        mutation,
    };
    LocalControlClient::send_metadata(&owner, &request)
        .map(|_| true)
        .map_err(|error| map_error(error, &owner))
}

pub(super) fn sync(context: &mut RuntimeContext, session_id: SessionId) -> Result<(), CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(());
    };
    let mutation = ControlMutation::Sync;
    let protocol = required_protocol(&owner, &mutation)?;
    let request = ControlRequest {
        protocol,
        request_id: context.ids.request_id(),
        session_id,
        mutation,
    };
    match LocalControlClient::send_metadata(&owner, &request) {
        Ok(crate::ports::control::ControlMetadataReceipt::Synchronized) => Ok(()),
        Ok(crate::ports::control::ControlMetadataReceipt::SessionRenamed { .. }) => {
            Err(CliError::new(
                "protocol_mismatch",
                "owner returned a rename receipt".to_owned(),
                6,
            ))
        }
        Err(error) => Err(map_error(error, &owner)),
    }
}

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

pub(super) fn replace(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    replacement: String,
    expected_digest: Option<[u8; 32]>,
    revision_id: RevisionId,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let mutation = ControlMutation::Replace {
        revision_id,
        thought_id,
        expected_digest,
        content: replacement,
    };
    let receipt = send(context, &owner, session_id, mutation)?;
    Ok(Some(ThoughtMutation {
        thought_id,
        receipt,
    }))
}

pub(super) fn set_collapsed(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    collapsed: bool,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let mutation = ControlMutation::SetCollapsed {
        operation_id,
        thought_id,
        collapsed,
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
    let protocol = required_protocol(owner, &mutation)?;
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

fn required_protocol(owner: &InstanceInfo, mutation: &ControlMutation) -> Result<u32, CliError> {
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
        || protocol < mutation.minimum_protocol()
    {
        return Err(CliError::new(
            "session_busy",
            "active owner does not support the required control protocol".to_owned(),
            5,
        ));
    }
    Ok(protocol)
}

fn map_error(error: ControlError, owner: &InstanceInfo) -> CliError {
    let details = json!({
        "session_id": owner.session_id,
        "holder": owner,
    });
    match error {
        ControlError::Rejected { code, message } => match code.as_str() {
            "thought_not_found" => CliError::new("thought_not_found", message, 3),
            "content_conflict" => CliError::new("content_conflict", message, 7),
            "thought_locked" => CliError::new("thought_locked", message, 7),
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
