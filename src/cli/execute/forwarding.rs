//! Active-session CLI mutations forwarded to the verified reducer owner.

use crate::cli::error_code::ErrorCode;
mod board;
pub(super) use board::{
    extract_thought, insert_separator, merge_thoughts, move_item, mutate_items, reflow_thought,
    split_thought,
};

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

struct ForwardedAdd<'a> {
    body: &'a str,
    annotations: Vec<ContentAnnotation>,
    name: Option<crate::domain::ThoughtName>,
    position: Option<usize>,
    supplied: Option<OperationId>,
    preserve: bool,
}

pub(super) fn rename_session(
    context: &mut RuntimeContext,
    session_id: SessionId,
    name: Option<String>,
    operation_id: OperationId,
) -> Result<bool, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(false);
    };
    let mutation = ControlMutation::RenameSession { operation_id, name };
    let protocol = required_protocol(&owner, &mutation)?;
    let request = ControlRequest {
        protocol,
        request_id: context.ids.request_id(),
        session_id,
        mutation,
    };
    LocalControlClient::send_metadata(&owner, &request)
        .map(|_| true)
        .map_err(|error| map_error(error, &owner, false))
}

pub(super) fn sync(context: &mut RuntimeContext, session_id: SessionId) -> Result<(), CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(());
    };
    let mutation = ControlMutation::Sync;
    let Some(protocol) = sync_protocol(owner.control_protocol)? else {
        return Ok(());
    };
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
                ErrorCode::ProtocolMismatch,
                "owner returned a rename receipt".to_owned(),
            ))
        }
        Err(error) => Err(map_error(error, &owner, false)),
    }
}

pub(super) fn add(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    add_with_kind(
        context,
        session_id,
        ForwardedAdd {
            body,
            annotations: Vec::new(),
            name: None,
            position,
            supplied,
            preserve: false,
        },
    )
}

pub(super) fn preserve_add(
    context: &mut RuntimeContext,
    session_id: SessionId,
    body: &str,
    annotations: Vec<ContentAnnotation>,
    name: Option<crate::domain::ThoughtName>,
    position: Option<usize>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    add_with_kind(
        context,
        session_id,
        ForwardedAdd {
            body,
            annotations,
            name,
            position,
            supplied,
            preserve: true,
        },
    )
}

fn add_with_kind(
    context: &mut RuntimeContext,
    session_id: SessionId,
    add: ForwardedAdd<'_>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = add.supplied.unwrap_or_else(|| context.ids.operation_id());
    let thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|error| CliError::identifier(error.to_string()))?;
    let mutation = if add.preserve {
        ControlMutation::PreserveAdd {
            operation_id,
            thought_id,
            content: add.body.to_owned(),
            annotations: add.annotations,
            name: add.name,
            position: add.position,
        }
    } else {
        ControlMutation::Add {
            operation_id,
            thought_id,
            content: add.body.to_owned(),
            annotations: Vec::new(),
            position: add.position,
        }
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

pub(super) fn rename_thought(
    context: &mut RuntimeContext,
    session_id: SessionId,
    thought_id: ThoughtId,
    name: Option<crate::domain::ThoughtName>,
    supplied: Option<OperationId>,
) -> Result<Option<ThoughtMutation>, CliError> {
    let Some(owner) = owner(context, session_id)? else {
        return Ok(None);
    };
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let receipt = send(
        context,
        &owner,
        session_id,
        ControlMutation::RenameThought {
            operation_id,
            thought_id,
            name,
        },
    )?;
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
    send_control(context, owner, session_id, mutation).map(|receipt| receipt.durable)
}

fn send_control(
    context: &mut RuntimeContext,
    owner: &InstanceInfo,
    session_id: SessionId,
    mutation: ControlMutation,
) -> Result<crate::ports::control::ControlReceipt, CliError> {
    let reports_invalid_state = matches!(
        mutation,
        ControlMutation::InsertSeparator { .. }
            | ControlMutation::DeleteItems { .. }
            | ControlMutation::MoveItem { .. }
            | ControlMutation::DuplicateItems { .. }
            | ControlMutation::SplitThought { .. }
            | ControlMutation::ExtractThought { .. }
            | ControlMutation::MergeThoughts { .. }
            | ControlMutation::ReflowThought { .. }
    );
    let protocol = required_protocol(owner, &mutation)?;
    let request = ControlRequest {
        protocol,
        request_id: context.ids.request_id(),
        session_id,
        mutation,
    };
    LocalControlClient
        .send(owner, &request)
        .map_err(|error| map_error(error, owner, reports_invalid_state))
}

fn required_protocol(owner: &InstanceInfo, mutation: &ControlMutation) -> Result<u32, CliError> {
    let protocol = owner.control_protocol.ok_or_else(|| {
        CliError::new(
            ErrorCode::SessionBusy,
            "active owner does not advertise a control protocol".to_owned(),
        )
    })?;
    if !(crate::ports::control::MIN_CONTROL_PROTOCOL_VERSION
        ..=crate::ports::control::CONTROL_PROTOCOL_VERSION)
        .contains(&protocol)
        || protocol < mutation.minimum_protocol()
    {
        return Err(CliError::new(
            ErrorCode::SessionBusy,
            "active owner does not support the required control protocol".to_owned(),
        ));
    }
    Ok(protocol)
}

fn sync_protocol(advertised: Option<u32>) -> Result<Option<u32>, CliError> {
    let Some(protocol) = advertised else {
        return Ok(None);
    };
    if !(crate::ports::control::MIN_CONTROL_PROTOCOL_VERSION
        ..=crate::ports::control::CONTROL_PROTOCOL_VERSION)
        .contains(&protocol)
    {
        return Err(CliError::new(
            ErrorCode::SessionBusy,
            "active owner advertises an unsupported control protocol".to_owned(),
        ));
    }
    Ok((protocol >= ControlMutation::Sync.minimum_protocol()).then_some(protocol))
}

fn map_error(error: ControlError, owner: &InstanceInfo, reports_invalid_state: bool) -> CliError {
    let details = json!({
        "session_id": owner.session_id,
        "holder": owner,
    });
    match error {
        ControlError::Rejected { code, message } => match code.as_str() {
            "thought_not_found" => CliError::new(ErrorCode::ThoughtNotFound, message),
            "content_conflict" => CliError::new(ErrorCode::ContentConflict, message),
            "thought_locked" => CliError::new(ErrorCode::ThoughtLocked, message),
            "storage_failed" => CliError::new(ErrorCode::StorageFailed, message),
            "storage_busy" => CliError::new(ErrorCode::StorageBusy, message),
            "storage_full" => CliError::new(ErrorCode::StorageFull, message),
            "idempotency_conflict" => CliError::new(ErrorCode::IdempotencyConflict, message),
            "no_durable_mutation" | "no_change" => CliError::new(ErrorCode::NoChange, message),
            "invalid_state" if reports_invalid_state => {
                CliError::new(ErrorCode::InvalidState, message)
            }
            "outcome_unknown" => {
                CliError::new(ErrorCode::OperationIndeterminate, message).with_details(details)
            }
            "owner_busy" | "protocol_mismatch" | "wrong_session" => {
                CliError::new(ErrorCode::SessionBusy, message).with_details(details)
            }
            _ => CliError::new(ErrorCode::MutationRejected, message),
        },
        ControlError::MessageTooLarge => CliError::input(error.to_string()),
        ControlError::Unsupported
        | ControlError::InvalidPeer
        | ControlError::Protocol(_)
        | ControlError::Timeout
        | ControlError::Io(_) => {
            CliError::new(ErrorCode::SessionBusy, error.to_string()).with_details(details)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sync_protocol;

    #[test]
    fn read_sync_degrades_for_legacy_owners_but_rejects_newer_protocols() {
        assert_eq!(
            sync_protocol(None).expect("unadvertised legacy owner"),
            None
        );
        assert_eq!(sync_protocol(Some(3)).expect("protocol three owner"), None);
        assert_eq!(
            sync_protocol(Some(4)).expect("protocol four owner"),
            Some(4)
        );
        assert_eq!(sync_protocol(Some(7)).expect("current owner"), Some(7));
        assert!(sync_protocol(Some(crate::ports::control::CONTROL_PROTOCOL_VERSION + 1)).is_err());
    }
}
