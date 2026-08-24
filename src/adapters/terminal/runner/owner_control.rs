//! Fair active-owner control routing through the live reducer.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::{control::ControlEnvelope, runtime::SystemClock, terminal::TerminalError},
    application::{ControlReplay, Effect, match_control_replay},
    domain::{RequestId, ThoughtId},
    ports::{control::ControlResult, store::StoredOperationRequest},
    ui::BoardApp,
};

use super::{
    PendingControl, PendingWork, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
    storage_error_code,
};

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
) -> Result<DrainOutcome, TerminalError> {
    let Some(control) = lanes.control else {
        return Ok(DrainOutcome::default());
    };
    drain_bounded(
        || match control.receiver.try_recv() {
            Ok(envelope) => Ok(Some(envelope)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(TerminalError::Worker("control request lane disconnected"))
            }
        },
        |envelope| queue_lookup(app, lanes, pending, ids, clock, envelope),
    )
}

fn queue_lookup(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    envelope: ControlEnvelope,
) -> Result<bool, TerminalError> {
    if app.quit {
        envelope.respond(ControlResult::Rejected {
            code: "owner_shutting_down".to_owned(),
            message: "active owner is shutting down; retry after the session becomes resumable"
                .to_owned(),
        });
        return Ok(false);
    }
    if envelope.request.session_id != app.state.board.session.id {
        envelope.respond(ControlResult::Rejected {
            code: "wrong_session".to_owned(),
            message: "request does not address the active owner session".to_owned(),
        });
        return Ok(false);
    }
    let effects = app.flush_pending_edit(ids, &clock);
    super::durability::enqueue_effects(app, lanes, effects, pending)?;
    let request_id = envelope.request.request_id;
    let operation_id = envelope.request.mutation.operation_id();
    match lanes.persistence.lookup(request_id, operation_id) {
        Ok(()) => {
            pending.persistence = pending.persistence.saturating_add(1);
            pending.control_lookups.insert(request_id, envelope);
        }
        Err(error) => envelope.respond(ControlResult::Rejected {
            code: "owner_busy".to_owned(),
            message: error.to_string(),
        }),
    }
    Ok(false)
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
            code: "idempotency_conflict".to_owned(),
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
            code: "no_durable_mutation".to_owned(),
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
            code: "storage_failed".to_owned(),
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
