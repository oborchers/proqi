//! Persistence effect routing and ordered acknowledgements.

use std::sync::mpsc::TryRecvError;

use crate::{
    application::Effect,
    domain::OperationSequence,
    ports::control::{ControlReceipt, ControlResult},
    ui::BoardApp,
};

use super::{PendingWork, WorkerLanes, storage_error_code};
use crate::adapters::terminal::{
    TerminalError, integration::integration_context, persistence::PersistenceResult,
};

pub(super) fn enqueue_effects(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    effects: Vec<Effect>,
    pending: &mut PendingWork,
) -> Result<(), TerminalError> {
    for effect in effects {
        if let Some(batch) = effect.persistence_batch() {
            let sequence = batch
                .sequence()
                .ok_or(TerminalError::Worker("mutable batch lacks sequence"))?;
            if let Err(error) = lanes.persistence.commit(batch) {
                app.acknowledge_persistence(sequence, false);
                return Err(error);
            }
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::RetryPersistence { sequence } = effect {
            lanes.persistence.retry(sequence)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::StoreIntegrationContext {
            session_id,
            target,
            verified_at,
        } = effect
        {
            lanes.persistence.metadata(
                crate::ports::store::OperationBatch::IntegrationContext {
                    session_id,
                    context: Some(integration_context(&target, verified_at)),
                },
            )?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if lanes.external.send(&effect)? {
            pending.external = pending.external.saturating_add(1);
        } else if let Effect::Notify { code } = effect {
            app.notify(code);
        }
    }
    Ok(())
}

pub(super) fn drain_persistence(
    app: &mut BoardApp,
    persistence: &crate::adapters::terminal::persistence::PersistenceLane,
    pending: &mut PendingWork,
) -> Result<bool, TerminalError> {
    let mut changed = false;
    loop {
        match persistence.receiver.try_recv() {
            Ok(PersistenceResult::Sequenced {
                sequence,
                result,
                retried,
            }) => {
                changed = true;
                if !retried {
                    pending.persistence = pending.persistence.saturating_sub(1);
                }
                let succeeded = complete_sequence(app, pending, sequence, result);
                app.acknowledge_persistence(sequence, succeeded);
            }
            Ok(PersistenceResult::RetryFinished) => {
                changed = true;
                pending.persistence = pending.persistence.saturating_sub(1);
            }
            Ok(PersistenceResult::Metadata { result }) => {
                changed = true;
                pending.persistence = pending.persistence.saturating_sub(1);
                if let Err(error) = result {
                    app.status = Some(format!(
                        "submission accepted, but integration context was not saved: {error}"
                    ));
                }
            }
            Err(TryRecvError::Empty) => return Ok(changed),
            Err(TryRecvError::Disconnected) if pending.persistence == 0 => return Ok(changed),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker(
                    "persistence result lane disconnected",
                ));
            }
        }
    }
}

fn complete_sequence(
    app: &mut BoardApp,
    pending: &mut PendingWork,
    sequence: OperationSequence,
    result: Result<crate::ports::store::CommitReceipt, crate::ports::store::StoreError>,
) -> bool {
    match result {
        Ok(receipt) => {
            complete_control(
                pending,
                sequence,
                ControlResult::Accepted(ControlReceipt {
                    thought_id: pending
                        .controls
                        .get(&sequence)
                        .and_then(|control| control.thought_id),
                    durable: receipt,
                }),
            );
            true
        }
        Err(error) => {
            complete_control(
                pending,
                sequence,
                ControlResult::Rejected {
                    code: storage_error_code(&error).to_owned(),
                    message: error.to_string(),
                },
            );
            app.status = Some(format!("{error}; press r to retry or w to export recovery"));
            false
        }
    }
}

fn complete_control(pending: &mut PendingWork, sequence: OperationSequence, result: ControlResult) {
    if let Some(control) = pending.controls.remove(&sequence) {
        control.envelope.respond(result);
    }
}
