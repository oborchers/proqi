//! Fair active-owner control routing through the live reducer.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::{control::ControlEnvelope, runtime::SystemClock, terminal::TerminalError},
    application::{ControlReplay, Effect, match_control_replay},
    domain::{RequestId, ThoughtId},
    ports::{
        control::{ControlMutation, ControlRejectionCode, ControlResult, ControlUpdateReceipt},
        environment::Clock as _,
        store::StoredOperationRequest,
        update::{UpdatePrepareReply, UpdateRestartReply},
    },
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
    let mut outcome = drain_bounded(
        || match control.receiver.try_recv() {
            Ok(envelope) => Ok(Some(envelope)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if control.is_stopping() => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(TerminalError::Worker("control request lane disconnected"))
            }
        },
        |envelope| queue_lookup(app, lanes, pending, ids, clock, envelope),
    )?;
    outcome.changed |= complete_update_prepares(app, pending, lanes.instance, clock);
    Ok(outcome)
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
            code: ControlRejectionCode::OwnerShuttingDown.as_str().to_owned(),
            message: "active owner is shutting down; retry after the session becomes resumable"
                .to_owned(),
        });
        return Ok(false);
    }
    if envelope.request.session_id != app.state.board.session.id {
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::WrongSession.as_str().to_owned(),
            message: "request does not address the active owner session".to_owned(),
        });
        return Ok(false);
    }
    if matches!(
        envelope.request.mutation,
        ControlMutation::UpdatePrepare { .. }
            | ControlMutation::UpdateRelease { .. }
            | ControlMutation::UpdateRestart { .. }
    ) {
        return handle_update(app, lanes, pending, ids, clock, envelope);
    }
    let effects = app.flush_pending_edit(ids, &clock);
    super::durability::enqueue_effects(app, lanes, effects, pending)?;
    let request_id = envelope.request.request_id;
    let Some(operation_id) = envelope.request.mutation.durable_operation_id() else {
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::InvalidControlRequest
                .as_str()
                .to_owned(),
            message: "update request reached the durable mutation lane".to_owned(),
        });
        return Ok(false);
    };
    match lanes.persistence.lookup(request_id, operation_id) {
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

fn handle_update(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    envelope: ControlEnvelope,
) -> Result<bool, TerminalError> {
    match envelope.request.mutation.clone() {
        ControlMutation::UpdatePrepare { request } => {
            queue_update_prepare(app, lanes, pending, ids, clock, envelope, &request)
        }
        ControlMutation::UpdateRelease { operation_id } => {
            let released = app.release_update_barrier(operation_id);
            respond_update_release(envelope, lanes.instance.instance_id, released);
            Ok(released)
        }
        ControlMutation::UpdateRestart { request } => {
            let accepted =
                app.request_update_restart(request.operation_id, request.installed_version);
            envelope.respond(ControlResult::Update(ControlUpdateReceipt::Restart(
                UpdateRestartReply {
                    instance_id: lanes.instance.instance_id,
                    accepted,
                },
            )));
            Ok(accepted)
        }
        ControlMutation::Add { .. }
        | ControlMutation::Delete { .. }
        | ControlMutation::Move { .. }
        | ControlMutation::History { .. } => Ok(false),
    }
}

fn queue_update_prepare(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    envelope: ControlEnvelope,
    request: &crate::ports::update::UpdatePrepareRequest,
) -> Result<bool, TerminalError> {
    let verified = lanes.instance.update.as_ref().is_some_and(|context| {
        context.installation_identity == request.installation_identity
            && context.protocol == crate::ports::update::UPDATE_CONTROL_PROTOCOL_VERSION
    });
    if !verified || clock.now() >= request.deadline {
        envelope.respond(ControlResult::Update(ControlUpdateReceipt::Prepared(
            UpdatePrepareReply::Blocked {
                instance_id: lanes.instance.instance_id,
                code: if verified {
                    "deadline_expired"
                } else {
                    "installation_mismatch"
                }
                .to_owned(),
            },
        )));
        return Ok(false);
    }
    if !app.begin_update_barrier(request.operation_id, request.deadline) {
        envelope.respond(ControlResult::Update(ControlUpdateReceipt::Prepared(
            UpdatePrepareReply::Blocked {
                instance_id: lanes.instance.instance_id,
                code: ControlRejectionCode::AnotherUpdateIsPreparing
                    .as_str()
                    .to_owned(),
            },
        )));
        return Ok(false);
    }
    let effects = app.flush_pending_edit(ids, &clock);
    super::durability::enqueue_effects(app, lanes, effects, pending)?;
    pending
        .update_prepares
        .insert(envelope.request.request_id, envelope);
    Ok(true)
}

fn complete_update_prepares(
    app: &mut BoardApp,
    pending: &mut PendingWork,
    instance: &crate::ports::runtime::InstanceInfo,
    clock: SystemClock,
) -> bool {
    if pending.persistence > 0 || pending.update_prepares.is_empty() {
        return false;
    }
    let request_ids: Vec<_> = pending.update_prepares.keys().copied().collect();
    let mut changed = false;
    for request_id in request_ids {
        let Some(envelope) = pending.update_prepares.remove(&request_id) else {
            continue;
        };
        let ControlMutation::UpdatePrepare { request } = &envelope.request.mutation else {
            continue;
        };
        let blocked = if clock.now() >= request.deadline {
            Some("deadline_expired")
        } else if app.update_preflight_failed() {
            Some("save_failed")
        } else if !app.update_preflight_ready() {
            pending.update_prepares.insert(request_id, envelope);
            continue;
        } else {
            None
        };
        let reply = blocked.map_or_else(
            || UpdatePrepareReply::Ready {
                instance_id: instance.instance_id,
                session_id: instance.session_id,
            },
            |code| UpdatePrepareReply::Blocked {
                instance_id: instance.instance_id,
                code: code.to_owned(),
            },
        );
        if blocked.is_some() {
            app.release_update_barrier(request.operation_id);
        }
        envelope.respond(ControlResult::Update(ControlUpdateReceipt::Prepared(reply)));
        changed = true;
    }
    changed
}

fn respond_update_release(
    envelope: ControlEnvelope,
    instance_id: crate::domain::InstanceId,
    released: bool,
) {
    if released {
        envelope.respond(ControlResult::Update(ControlUpdateReceipt::Released {
            instance_id,
        }));
    } else {
        envelope.respond(ControlResult::Rejected {
            code: ControlRejectionCode::UpdateOperationMismatch
                .as_str()
                .to_owned(),
            message: "participant is not waiting for that update".to_owned(),
        });
    }
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
