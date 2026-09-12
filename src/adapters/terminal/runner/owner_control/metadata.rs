//! Request-correlated session metadata and synchronization receipts.

use crate::{
    adapters::{control::ControlEnvelope, terminal::TerminalError},
    application::Effect,
    domain::RequestId,
    ports::{
        control::{ControlMetadataReceipt, ControlMutation, ControlRejectionCode, ControlResult},
        store::StoreError,
    },
    ui::BoardApp,
};

use super::super::{PendingWork, WorkerLanes, storage_error_code};

pub(super) fn queue(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    envelope: ControlEnvelope,
    clock: &impl crate::ports::environment::Clock,
) -> Result<bool, TerminalError> {
    match app.handle_control(&envelope.request.mutation, clock) {
        Ok(effects) => queue_rename(lanes, pending, envelope, &effects),
        Err(error) => {
            envelope.respond(ControlResult::Rejected {
                code: error.code().as_str().to_owned(),
                message: error.to_string(),
            });
            Ok(false)
        }
    }
}

fn queue_rename(
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    envelope: ControlEnvelope,
    effects: &[Effect],
) -> Result<bool, TerminalError> {
    if effects.is_empty() {
        let ControlMutation::RenameSession { operation_id, .. } = envelope.request.mutation else {
            return Err(TerminalError::Worker("metadata control was not a rename"));
        };
        let request_id = envelope.request.request_id;
        lanes.persistence.browser_lookup(request_id, operation_id)?;
        pending.persistence = pending.persistence.saturating_add(1);
        pending.metadata_controls.insert(request_id, envelope);
        return Ok(true);
    }
    let [Effect::CommitBrowserOperation(operation)] = effects else {
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::InvalidControlRequest
                .as_str()
                .to_owned(),
            message: "rename did not produce one Browser operation".to_owned(),
        });
        return Ok(false);
    };
    let request_id = envelope.request.request_id;
    let previous_name = match operation.inverse() {
        crate::domain::BrowserMutation::SetName { value, .. } => value.clone(),
        crate::domain::BrowserMutation::SetDeletedAt { .. } => None,
    };
    lanes
        .persistence
        .browser_operation(Some(request_id), previous_name, operation.clone())?;
    pending.persistence = pending.persistence.saturating_add(1);
    pending.metadata_controls.insert(request_id, envelope);
    Ok(true)
}

pub(in crate::adapters::terminal::runner) fn complete_lookup(
    pending: &mut PendingWork,
    request_id: RequestId,
    result: Result<Option<crate::domain::BrowserOperation>, StoreError>,
) {
    let Some(envelope) = pending.metadata_controls.remove(&request_id) else {
        return;
    };
    let response = match result {
        Ok(Some(operation)) if rename_matches(&envelope, &operation) => renamed(&envelope),
        Ok(Some(_)) => ControlResult::Rejected {
            code: ControlRejectionCode::IdempotencyConflict
                .as_str()
                .to_owned(),
            message: "operation identity belongs to another Browser request".to_owned(),
        },
        Ok(None) => renamed(&envelope),
        Err(error) => ControlResult::Rejected {
            code: super::storage_error_code(&error).to_owned(),
            message: error.to_string(),
        },
    };
    envelope.respond(response);
}

fn rename_matches(envelope: &ControlEnvelope, operation: &crate::domain::BrowserOperation) -> bool {
    let ControlMutation::RenameSession { operation_id, name } = &envelope.request.mutation else {
        return false;
    };
    operation.id() == *operation_id
        && operation.session_id() == envelope.request.session_id
        && operation.kind() == crate::domain::BrowserOperationKind::Rename
        && matches!(
            operation.forward(),
            crate::domain::BrowserMutation::SetName { value, .. } if value == name
        )
}

pub(in crate::adapters::terminal::runner) fn complete(
    pending: &mut PendingWork,
    request_id: Option<RequestId>,
    result: &Result<(), StoreError>,
) {
    let Some(request_id) = request_id else {
        return;
    };
    let Some(envelope) = pending.metadata_controls.remove(&request_id) else {
        return;
    };
    let response = match result {
        Ok(()) => renamed(&envelope),
        Err(error) => ControlResult::Rejected {
            code: storage_error_code(error).to_owned(),
            message: error.to_string(),
        },
    };
    envelope.respond(response);
}

fn renamed(envelope: &ControlEnvelope) -> ControlResult {
    let name = match &envelope.request.mutation {
        ControlMutation::RenameSession { name, .. } => name.clone(),
        _ => None,
    };
    ControlResult::Metadata(ControlMetadataReceipt::SessionRenamed { name })
}

pub(in crate::adapters::terminal::runner) fn complete_sync(pending: &mut PendingWork) {
    if pending.persistence > 0 {
        return;
    }
    while let Some(envelope) = pending.sync_controls.pop_front() {
        envelope.respond(ControlResult::Metadata(
            ControlMetadataReceipt::Synchronized,
        ));
    }
}
