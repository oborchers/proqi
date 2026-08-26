//! Persistence effect routing and ordered acknowledgements.

use std::sync::mpsc::TryRecvError;

use crate::{
    application::Effect,
    domain::OperationSequence,
    ports::control::{ControlReceipt, ControlResult},
    ui::BoardApp,
};

use super::{
    PendingWork, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
    owner_control, storage_error_code,
};
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
        } else if let Effect::RenameSession {
            session_id,
            previous_name,
            name,
        } = effect
        {
            lanes
                .persistence
                .rename_session(session_id, previous_name, name)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::DiscoverTransferSessions = effect {
            lanes
                .persistence
                .discover_transfer_sessions(app.state.board.session.id)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::TransferThought(request) = effect {
            lanes.persistence.transfer_thought(request)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::PrepareSubmission(attempt) = effect {
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::Submission {
                    submission_id: attempt.id,
                    state: "preparing",
                    direction: attempt.direction,
                    provider: super::diagnostics::provider_name(&attempt.provider),
                    outcome: None,
                },
            );
            lanes.persistence.prepare_submission(attempt)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::MarkSubmissionSending { submission_id, at } = effect {
            lanes
                .persistence
                .mark_submission_sending(submission_id, at)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::FinishSubmission {
            submission_id,
            outcome,
        } = effect
        {
            lanes
                .persistence
                .finish_submission(submission_id, outcome)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if lanes.update.send(&effect)? {
            pending.update = pending.update.saturating_add(1);
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
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut impl crate::ports::environment::IdGenerator,
    clock: &impl crate::ports::environment::Clock,
) -> Result<DrainOutcome, TerminalError> {
    drain_bounded(
        || match lanes.persistence.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if lanes.persistence.stopped_cleanly() => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(lanes
                    .persistence
                    .worker_failure()
                    .unwrap_or(TerminalError::Worker(
                        "persistence result lane disconnected",
                    )))
            }
        },
        |result| complete_result(app, lanes, pending, ids, clock, result),
    )
}

fn complete_result(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut impl crate::ports::environment::IdGenerator,
    clock: &impl crate::ports::environment::Clock,
    result: PersistenceResult,
) -> Result<bool, TerminalError> {
    match result {
        PersistenceResult::Sequenced {
            sequence,
            result,
            retried,
        } => {
            if !retried {
                pending.persistence = pending.persistence.saturating_sub(1);
            }
            let application_result = complete_sequence(app, pending, sequence, result);
            app.acknowledge_persistence_result(sequence, application_result);
        }
        PersistenceResult::RetryFinished => {
            pending.persistence = pending.persistence.saturating_sub(1);
        }
        PersistenceResult::Metadata { result } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            if let Err(error) = result {
                app.set_warning(format!(
                    "submission accepted, but integration context was not saved: {error}"
                ));
            }
        }
        PersistenceResult::SessionRenamed {
            previous_name,
            result,
        } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            owner_control::complete_metadata(pending, &result);
            app.complete_session_rename(previous_name, result);
        }
        PersistenceResult::TransferSessions(result) => {
            pending.persistence = pending.persistence.saturating_sub(1);
            app.complete_transfer_discovery(result);
        }
        PersistenceResult::ThoughtTransferred { request, result } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            let effects = app.complete_session_transfer(&request, result, ids, clock);
            enqueue_effects(app, lanes, effects, pending)?;
        }
        PersistenceResult::Lookup { request_id, result } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            return owner_control::complete_lookup(app, lanes, pending, clock, request_id, result);
        }
        PersistenceResult::SubmissionPrepared {
            submission_id,
            result,
        } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            record_submission_result(submission_id, "prepared", &result);
            let effects = app.complete_submission_prepared(submission_id, result);
            enqueue_effects(app, lanes, effects, pending)?;
        }
        PersistenceResult::SubmissionSending {
            submission_id,
            result,
        } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            record_submission_result(submission_id, "sending", &result);
            let effects = app.complete_submission_sending(submission_id, result);
            enqueue_effects(app, lanes, effects, pending)?;
        }
        PersistenceResult::SubmissionFinished {
            submission_id,
            result,
        } => {
            pending.persistence = pending.persistence.saturating_sub(1);
            record_submission_result(submission_id, "journaled", &result);
            let effects = app.complete_submission_journaled(submission_id, result);
            enqueue_effects(app, lanes, effects, pending)?;
        }
    }
    owner_control::complete_sync(pending);
    Ok(true)
}

fn record_submission_result(
    submission_id: crate::domain::SubmissionId,
    state: &'static str,
    result: &Result<(), crate::ports::store::StoreError>,
) {
    crate::adapters::diagnostics::record(
        crate::adapters::diagnostics::SafeEvent::SubmissionState {
            submission_id,
            state,
            outcome: result.as_ref().err().map(storage_error_code),
        },
    );
}

fn complete_sequence(
    app: &mut BoardApp,
    pending: &mut PendingWork,
    sequence: OperationSequence,
    result: Result<crate::ports::store::CommitReceipt, crate::ports::store::StoreError>,
) -> Result<(), crate::application::FailureCode> {
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
            Ok(())
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
            if error == crate::ports::store::StoreError::RecoveryCapacity {
                app.set_error(format!("{error}; press w to export recovery"));
                Err(crate::application::FailureCode::RecoveryCapacity)
            } else {
                app.set_error(format!("{error}; press r to retry or w to export recovery"));
                Err(crate::application::FailureCode::StorageFailed)
            }
        }
    }
}

fn complete_control(pending: &mut PendingWork, sequence: OperationSequence, result: ControlResult) {
    if let Some(control) = pending.controls.remove(&sequence) {
        control.envelope.respond(result);
    }
}
