//! Retry-safe session rename, trash, restore, prune, and Browser history commands.

use serde_json::{Value, json};

use crate::{
    application::{BrowserHistoryMovement, SessionAdministrationReceipt},
    domain::{BrowserOperationKind, SessionId, validate_session_name},
    ports::{environment::IdGenerator, store::SessionRequest},
};

use super::{
    super::{forwarding, helpers::parse_operation_id},
    CliError, Outcome, RuntimeContext, session_service,
};

#[derive(Clone, Copy)]
pub(super) enum Management {
    Trash,
    Restore,
    Prune,
}

impl Management {
    const fn label(self) -> &'static str {
        match self {
            Self::Trash => "trashed",
            Self::Restore => "restored",
            Self::Prune => "pruned",
        }
    }
}

pub(super) fn rename(
    context: &mut RuntimeContext,
    session: &str,
    name: Option<&str>,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let supplied = parse_operation_id(operation)?;
    let name = name.map(str::to_owned);
    if let Some(name) = name.as_deref() {
        validate_session_name(name).map_err(|error| CliError::input(error.to_string()))?;
    }
    let mut service = session_service(context)?;
    let id = service.resolve_session(session, true)?;
    let operation_id = supplied.unwrap_or_else(|| context.ids.operation_id());
    let request = SessionRequest::Rename {
        session_id: id,
        name: name.clone(),
    };
    let mut service = session_service(context)?;
    if service
        .session_request_replay(operation_id, &request)?
        .is_some()
    {
        return Ok(administration_outcome(
            &SessionAdministrationReceipt {
                session_id: id,
                operation_id,
                idempotent_replay: true,
                changed: false,
            },
            "renamed",
        ));
    }
    let current = service.inspect_session(id)?.board.session.name;
    drop(service);
    if forwarding::rename_session(context, id, name.clone(), operation_id)? {
        let receipt = SessionAdministrationReceipt {
            session_id: id,
            operation_id,
            idempotent_replay: false,
            changed: current != name,
        };
        return Ok(administration_outcome(&receipt, "renamed"));
    }
    let receipt =
        session_service(context)?.rename_session(id, name.as_deref(), Some(operation_id))?;
    Ok(administration_outcome(&receipt, "renamed"))
}

pub(super) fn manage(
    context: &mut RuntimeContext,
    reference: &str,
    action: Management,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let supplied = parse_operation_id(operation)?;
    let mut service = session_service(context)?;
    let id = service.resolve_session(reference, true)?;
    let receipt = match action {
        Management::Trash => service.trash_session(id, supplied)?,
        Management::Restore => service.restore_session(id, supplied)?,
        Management::Prune => service.prune_session(id, supplied)?,
    };
    Ok(administration_outcome(&receipt, action.label()))
}

pub(super) fn move_history(
    context: &mut RuntimeContext,
    undo: bool,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let supplied = parse_operation_id(operation)?;
    let movement = session_service(context)?.move_browser_history(undo, supplied)?;
    Ok(history_outcome(&movement, undo))
}

fn administration_outcome(receipt: &SessionAdministrationReceipt, status: &str) -> Outcome {
    let suffix = if receipt.idempotent_replay {
        " (replay)"
    } else if receipt.changed {
        ""
    } else {
        " (unchanged)"
    };
    Outcome {
        data: json!({
            "session_id": receipt.session_id,
            "status": status,
            "changed": receipt.changed,
            "receipt": receipt_json(receipt.session_id, receipt),
        }),
        human: format!("Session {} {status}{suffix}", receipt.session_id),
    }
}

fn receipt_json(session_id: SessionId, receipt: &SessionAdministrationReceipt) -> Value {
    json!({
        "session_id": session_id,
        "operation_id": receipt.operation_id,
        "idempotent_replay": receipt.idempotent_replay,
    })
}

fn history_outcome(movement: &BrowserHistoryMovement, undo: bool) -> Outcome {
    let action = if undo { "undid" } else { "redid" };
    let label = movement.target.map(BrowserOperationKind::label);
    Outcome {
        data: json!({
            "history": if undo { "undo" } else { "redo" },
            "operation": label,
            "cursor": movement.cursor,
            "receipt": {
                "operation_id": movement.operation_id,
                "idempotent_replay": movement.idempotent_replay,
            },
        }),
        human: format!(
            "{action} {}{}",
            label.unwrap_or("Browser history entry"),
            if movement.idempotent_replay {
                " (replay)"
            } else {
                ""
            }
        ),
    }
}
