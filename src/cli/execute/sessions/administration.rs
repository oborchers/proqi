//! Retry-safe session rename, trash, restore, prune, and Browser history commands.

use serde_json::{Value, json};

use crate::{
    application::{BrowserHistoryMovement, RenameAdmission, SessionAdministrationReceipt},
    domain::{BrowserOperationKind, SessionId},
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
    let mut service = session_service(context)?;
    let id = service.resolve_session(session, true)?;
    let admitted = match service.admit_rename(id, name, supplied)? {
        RenameAdmission::Replayed(receipt) => {
            return Ok(administration_outcome(&receipt, "renamed"));
        }
        RenameAdmission::New(receipt) => receipt,
    };
    drop(service);
    let operation_id = admitted.operation_id;
    if forwarding::rename_session(context, id, name.map(str::to_owned), operation_id)? {
        return Ok(administration_outcome(&admitted, "renamed"));
    }
    let receipt = session_service(context)?.rename_session(id, name, Some(operation_id))?;
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
