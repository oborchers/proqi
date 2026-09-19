//! Update control requests owned by the active terminal process.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::{
        control::{ControlDelivery, ControlEnvelope},
        runtime::SystemClock,
        terminal::TerminalError,
    },
    ports::{
        control::{ControlMutation, ControlRejectionCode, ControlResult, ControlUpdateReceipt},
        environment::Clock as _,
        update::{UpdatePrepareReply, UpdateQuiesceReply, UpdateRestartReply},
    },
    ui::{BoardApp, ScreenshotUpdateReadiness},
};

use super::super::{
    PendingWork, WorkerLanes,
    pending::{PendingUpdatePrepare, PendingUpdateRestart, UpdatePreparePhase},
};

pub(super) fn handle(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    schema_lease: &mut Option<crate::adapters::runtime::FileSchemaLease>,
    envelope: ControlEnvelope,
) -> Result<bool, TerminalError> {
    match envelope.request.mutation.clone() {
        ControlMutation::UpdatePrepare { request } => {
            queue_prepare(app, lanes, pending, ids, clock, envelope, &request)
        }
        ControlMutation::UpdateRelease { operation_id } => {
            let released = app.release_update_barrier(operation_id);
            respond_release(envelope, lanes.instance.instance_id, released);
            Ok(released)
        }
        ControlMutation::UpdateQuiesce { request } => {
            let quiesced =
                app.commit_update_quiescence(request.operation_id, &request.installed_version);
            if quiesced {
                schema_lease.take();
                envelope.respond(ControlResult::Update(ControlUpdateReceipt::Quiesced(
                    UpdateQuiesceReply {
                        instance_id: lanes.instance.instance_id,
                        session_id: lanes.instance.session_id,
                    },
                )));
            } else {
                envelope.respond(ControlResult::Rejected {
                    code: ControlRejectionCode::UpdateOperationMismatch
                        .as_str()
                        .to_owned(),
                    message: "participant is not prepared for that update".to_owned(),
                });
            }
            Ok(quiesced)
        }
        ControlMutation::UpdateRestart { request } => {
            Ok(queue_restart(app, lanes, pending, envelope, request))
        }
        ControlMutation::Add { .. }
        | ControlMutation::PreserveAdd { .. }
        | ControlMutation::RenameSession { .. }
        | ControlMutation::RenameThought { .. }
        | ControlMutation::InsertSeparator { .. }
        | ControlMutation::DeleteItems { .. }
        | ControlMutation::MoveItem { .. }
        | ControlMutation::DuplicateItems { .. }
        | ControlMutation::SplitThought { .. }
        | ControlMutation::ExtractThought { .. }
        | ControlMutation::MergeThoughts { .. }
        | ControlMutation::ReflowThought { .. }
        | ControlMutation::Sync
        | ControlMutation::Replace { .. }
        | ControlMutation::SetCollapsed { .. }
        | ControlMutation::Delete { .. }
        | ControlMutation::Move { .. }
        | ControlMutation::History { .. }
        | ControlMutation::CaptureTakeover { .. } => Ok(false),
    }
}

fn queue_restart(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    envelope: ControlEnvelope,
    request: crate::ports::update::UpdateRestartRequest,
) -> bool {
    let accepted = app.reserve_update_restart(request.operation_id, request.installed_version);
    let result = ControlResult::Update(ControlUpdateReceipt::Restart(UpdateRestartReply {
        instance_id: lanes.instance.instance_id,
        accepted,
    }));
    if !accepted {
        envelope.respond(result);
        return false;
    }
    let delivery = envelope.respond_confirmed(result);
    pending.update_restart = Some(PendingUpdateRestart {
        operation_id: request.operation_id,
        delivery,
    });
    if let Some(control) = lanes.control {
        control.request_stop();
    }
    false
}

pub(super) fn complete_restart(
    app: &mut BoardApp,
    pending: &mut PendingWork,
) -> Result<bool, TerminalError> {
    let Some(restart) = pending.update_restart.take() else {
        return Ok(false);
    };
    let delivered = match restart.delivery.try_recv() {
        Ok(ControlDelivery::Delivered) => true,
        Ok(ControlDelivery::Failed) | Err(TryRecvError::Disconnected) => false,
        Err(TryRecvError::Empty) => {
            pending.update_restart = Some(restart);
            return Ok(false);
        }
    };
    if !app.finish_update_restart_delivery(restart.operation_id, delivered) {
        return Err(TerminalError::Worker(
            "restart delivery did not match the reserved update",
        ));
    }
    Ok(true)
}

fn queue_prepare(
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
    pending.update_prepares.insert(
        envelope.request.request_id,
        PendingUpdatePrepare {
            envelope,
            phase: UpdatePreparePhase::AwaitingAdmission,
        },
    );
    complete_prepares(app, lanes, pending, ids, clock)
}

#[expect(
    clippy::too_many_lines,
    reason = "one typed update-preparation state machine keeps admission and replies atomic"
)]
pub(super) fn complete_prepares(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
) -> Result<bool, TerminalError> {
    if pending.update_prepares.is_empty() {
        return Ok(false);
    }
    let request_ids: Vec<_> = pending.update_prepares.keys().copied().collect();
    let mut changed = false;
    for request_id in request_ids {
        let Some(mut prepare) = pending.update_prepares.remove(&request_id) else {
            continue;
        };
        let ControlMutation::UpdatePrepare { request } = &prepare.envelope.request.mutation else {
            continue;
        };
        let operation_id = request.operation_id;
        let deadline = request.deadline;
        if clock.now() >= deadline {
            if prepare.phase == UpdatePreparePhase::AwaitingDurability {
                app.release_update_barrier(operation_id);
            }
            prepare
                .envelope
                .respond(ControlResult::Update(ControlUpdateReceipt::Prepared(
                    UpdatePrepareReply::Blocked {
                        instance_id: lanes.instance.instance_id,
                        code: "deadline_expired".to_owned(),
                    },
                )));
            changed = true;
            continue;
        }
        if prepare.phase == UpdatePreparePhase::AwaitingAdmission {
            match app.screenshot_update_readiness() {
                ScreenshotUpdateReadiness::CommitInFlight => {
                    pending.update_prepares.insert(request_id, prepare);
                    continue;
                }
                ScreenshotUpdateReadiness::Blocked => {
                    prepare.envelope.respond(ControlResult::Update(
                        ControlUpdateReceipt::Prepared(UpdatePrepareReply::Blocked {
                            instance_id: lanes.instance.instance_id,
                            code: "screenshot_not_quiescent".to_owned(),
                        }),
                    ));
                    changed = true;
                    continue;
                }
                ScreenshotUpdateReadiness::Ready => {}
            }
            if !app.pending_mutation_intents().is_empty() {
                pending.update_prepares.insert(request_id, prepare);
                continue;
            }
            if !app.begin_update_barrier(operation_id, request.target_version.clone(), deadline) {
                prepare
                    .envelope
                    .respond(ControlResult::Update(ControlUpdateReceipt::Prepared(
                        UpdatePrepareReply::Blocked {
                            instance_id: lanes.instance.instance_id,
                            code: ControlRejectionCode::AnotherUpdateIsPreparing
                                .as_str()
                                .to_owned(),
                        },
                    )));
                changed = true;
                continue;
            }
            let effects = app.flush_pending_edit(ids, &clock);
            super::super::durability::enqueue_effects(app, lanes, effects, pending)?;
            prepare.phase = UpdatePreparePhase::AwaitingDurability;
            pending.update_prepares.insert(request_id, prepare);
            changed = true;
            continue;
        }
        if pending.persistence > 0 {
            pending.update_prepares.insert(request_id, prepare);
            continue;
        }
        let blocked = if app.update_preflight_failed() {
            Some("save_failed")
        } else if !app.update_preflight_ready() {
            pending.update_prepares.insert(request_id, prepare);
            continue;
        } else {
            None
        };
        let reply = blocked.map_or_else(
            || UpdatePrepareReply::Ready {
                instance_id: lanes.instance.instance_id,
                session_id: lanes.instance.session_id,
            },
            |code| UpdatePrepareReply::Blocked {
                instance_id: lanes.instance.instance_id,
                code: code.to_owned(),
            },
        );
        if blocked.is_some() {
            app.release_update_barrier(operation_id);
        }
        prepare
            .envelope
            .respond(ControlResult::Update(ControlUpdateReceipt::Prepared(reply)));
        changed = true;
    }
    Ok(changed)
}

fn respond_release(
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
