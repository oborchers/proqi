//! Fair active-owner control routing through the live reducer.

mod capture;
mod metadata;
mod update;

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::{control::ControlEnvelope, runtime::SystemClock, terminal::TerminalError},
    application::{ControlReplay, Effect, match_control_replay},
    domain::{RequestId, ThoughtId},
    ports::{
        control::{ControlMutation, ControlRejectionCode, ControlResult},
        store::StoredOperationRequest,
    },
    ui::BoardApp,
};

use super::{
    CaptureRuntime, PendingControl, PendingWork, WorkerLanes, admission,
    fairness::{DrainOutcome, drain_bounded},
    storage_error_code,
};

pub(super) use metadata::{
    complete as complete_metadata, complete_noop_rename as complete_browser_noop_rename,
    complete_sync,
};

struct ControlContext<'a> {
    ids: &'a mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    schema_lease: &'a mut Option<crate::adapters::runtime::FileSchemaLease>,
}

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    schema_lease: &mut Option<crate::adapters::runtime::FileSchemaLease>,
) -> Result<DrainOutcome, TerminalError> {
    let Some(control) = lanes.control else {
        return Ok(DrainOutcome::default());
    };
    let mut context = ControlContext {
        ids,
        clock,
        schema_lease,
    };
    let mut outcome = drain_bounded(
        || match control.receiver.try_recv() {
            Ok(envelope) => Ok(Some(envelope)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if control.is_stopping() => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(TerminalError::Worker("control request lane disconnected"))
            }
        },
        |envelope| queue_lookup(app, lanes, pending, capture, &mut context, envelope),
    )?;
    outcome.changed |= capture::complete(lanes, pending, capture)?;
    outcome.changed |= update::complete_prepares(app, lanes, pending, context.ids, context.clock)?;
    outcome.changed |= update::complete_restart(app, pending)?;
    Ok(outcome)
}

fn queue_lookup(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    context: &mut ControlContext<'_>,
    envelope: ControlEnvelope,
) -> Result<bool, TerminalError> {
    if let Some(rejection) = owner_routing_rejection(app, &envelope) {
        envelope.respond(rejection);
        return Ok(false);
    }
    if matches!(
        envelope.request.mutation,
        ControlMutation::CaptureTakeover { .. }
    ) {
        return Ok(capture::queue(
            lanes.instance.instance_id,
            capture,
            envelope,
        ));
    }
    if let ControlMutation::UpdatePrepare { .. } = &envelope.request.mutation {
        return update::handle(
            app,
            lanes,
            pending,
            context.ids,
            context.clock,
            context.schema_lease,
            envelope,
        );
    }
    if let Some(rejection) = admission::owner_control_rejection(app, &envelope.request.mutation) {
        envelope.respond(rejection);
        return Ok(false);
    }
    if is_post_prepare_update(&envelope.request.mutation) {
        return update::handle(
            app,
            lanes,
            pending,
            context.ids,
            context.clock,
            context.schema_lease,
            envelope,
        );
    }
    let effects = app.flush_pending_edit(context.ids, &context.clock);
    super::durability::enqueue_effects(app, lanes, effects, pending)?;
    if matches!(
        envelope.request.mutation,
        ControlMutation::RenameSession { .. }
    ) {
        return metadata::queue(app, lanes, pending, envelope, &context.clock);
    }
    if matches!(envelope.request.mutation, ControlMutation::Sync) {
        pending.sync_controls.push_back(envelope);
        complete_sync(pending);
        return Ok(false);
    }
    let request_id = envelope.request.request_id;
    let Some(identity) = envelope.request.mutation.durable_identity() else {
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::InvalidControlRequest
                .as_str()
                .to_owned(),
            message: "update request reached the durable mutation lane".to_owned(),
        });
        return Ok(false);
    };
    match lanes.persistence.lookup(request_id, identity) {
        Ok(()) => {
            pending.persistence = pending.persistence.saturating_add(1);
            pending.control_lookups.insert(request_id, envelope);
        }
        Err(error) => envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::OwnerBusy.as_str().to_owned(),
            message: error.to_string(),
        }),
    }
    Ok(false)
}

fn is_post_prepare_update(mutation: &ControlMutation) -> bool {
    matches!(
        mutation,
        ControlMutation::UpdateRelease { .. }
            | ControlMutation::UpdateQuiesce { .. }
            | ControlMutation::UpdateRestart { .. }
    )
}

fn owner_routing_rejection(app: &BoardApp, envelope: &ControlEnvelope) -> Option<ControlResult> {
    if app.quit {
        return Some(ControlResult::Rejected {
            code: ControlRejectionCode::OwnerShuttingDown.as_str().to_owned(),
            message: "active owner is shutting down; retry after the session becomes resumable"
                .to_owned(),
        });
    }
    (envelope.request.session_id != app.state.board.session.id).then(|| ControlResult::Rejected {
        code: ControlRejectionCode::WrongSession.as_str().to_owned(),
        message: "request does not address the active owner session".to_owned(),
    })
}

pub(super) fn complete_lookup(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    clock: &impl crate::ports::environment::Clock,
    request_id: RequestId,
    result: Result<Option<StoredOperationRequest>, crate::ports::store::StoreError>,
) -> Result<bool, TerminalError> {
    let Some(envelope) = pending.control_lookups.remove(&request_id) else {
        return Err(TerminalError::Worker(
            "persistence returned an unknown control lookup",
        ));
    };
    match result {
        Err(error) => {
            envelope.respond(ControlResult::Rejected {
                code: storage_error_code(&error).to_owned(),
                message: error.to_string(),
            });
            Ok(false)
        }
        Ok(Some(existing)) => {
            respond_to_replay(envelope, &existing);
            Ok(false)
        }
        Ok(None) => apply_mutation(app, lanes, pending, envelope, clock),
    }
}

fn respond_to_replay(envelope: ControlEnvelope, existing: &StoredOperationRequest) {
    let result = match match_control_replay(
        existing,
        envelope.request.session_id,
        &envelope.request.mutation,
    ) {
        ControlReplay::Accepted(receipt) => ControlResult::Accepted(receipt),
        ControlReplay::Conflict => ControlResult::Rejected {
            code: ControlRejectionCode::IdempotencyConflict
                .as_str()
                .to_owned(),
            message: "operation identity belongs to another request".to_owned(),
        },
    };
    envelope.respond(result);
}

fn apply_mutation(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    envelope: ControlEnvelope,
    clock: &impl crate::ports::environment::Clock,
) -> Result<bool, TerminalError> {
    let thought_id = envelope.request.mutation.thought_id();
    match app.handle_control(&envelope.request.mutation, clock) {
        Ok(effects) => queue_effect(app, lanes, pending, envelope, thought_id, &effects),
        Err(error) => {
            envelope.respond(ControlResult::Rejected {
                code: error.code().as_str().to_owned(),
                message: error.to_string(),
            });
            Ok(false)
        }
    }
}

fn queue_effect(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    envelope: ControlEnvelope,
    thought_id: Option<ThoughtId>,
    effects: &[Effect],
) -> Result<bool, TerminalError> {
    let [effect] = effects else {
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::NoDurableMutation.as_str().to_owned(),
            message: "request produced no durable mutation".to_owned(),
        });
        return Ok(false);
    };
    let batch = effect
        .persistence_batch()
        .ok_or(TerminalError::Worker("control mutation lacked persistence"))?;
    let sequence = batch
        .sequence()
        .ok_or(TerminalError::Worker("control mutation lacked sequence"))?;
    if let Err(error) = lanes.persistence.commit(batch) {
        app.acknowledge_persistence(sequence, false);
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::StorageFailed.as_str().to_owned(),
            message: error.to_string(),
        });
        return Ok(true);
    }
    pending.persistence = pending.persistence.saturating_add(1);
    pending.controls.insert(
        sequence,
        PendingControl {
            envelope,
            thought_id,
        },
    );
    Ok(true)
}
