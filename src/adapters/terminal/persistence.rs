//! Ordered persistence lane with explicit retention for failed batches.

mod lane;
mod message;
mod retention;
mod transfer;

use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    adapters::{runtime::FileRuntimeCoordinator, sqlite::SqliteStore},
    domain::OperationSequence,
    ports::store::{OperationBatch, Store, StoreError},
};

use super::TerminalError;
use message::PersistenceRequest;
pub(super) use message::PersistenceResult;
pub(super) struct PersistenceLane {
    sender: Option<SyncSender<PersistenceRequest>>,
    pub(super) receiver: Receiver<PersistenceResult>,
    handle: Option<JoinHandle<()>>,
    lifecycle: super::supervisor::WorkerLifecycle,
}

fn persistence_loop(
    mut store: SqliteStore,
    mut runtime: Option<transfer::TransferRuntime>,
    requests: &Receiver<PersistenceRequest>,
    results: &SyncSender<PersistenceResult>,
) {
    let mut retained = BTreeMap::new();
    while let Ok(request) = requests.recv() {
        if !process_request(
            &mut store,
            runtime.as_mut(),
            request,
            &mut retained,
            results,
        ) {
            return;
        }
    }
}
fn process_request(
    store: &mut SqliteStore,
    runtime: Option<&mut transfer::TransferRuntime>,
    request: PersistenceRequest,
    retained: &mut BTreeMap<OperationSequence, Box<OperationBatch>>,
    results: &SyncSender<PersistenceResult>,
) -> bool {
    let (sequence, batch) = match request {
        PersistenceRequest::Commit(batch) => {
            let Some(sequence) = batch.sequence() else {
                return true;
            };
            (sequence, batch)
        }
        PersistenceRequest::Retry(sequence) => {
            return retry_from(store, sequence, retained, results);
        }
        PersistenceRequest::Metadata(batch) => {
            let result = store.commit(&batch).and_then(|receipt| {
                if receipt.is_none() {
                    Ok(())
                } else {
                    Err(StoreError::Integrity(
                        "metadata operation returned a durable receipt".to_owned(),
                    ))
                }
            });
            return results.send(PersistenceResult::Metadata { result }).is_ok();
        }
        PersistenceRequest::RenameSession {
            request_id,
            session_id,
            previous_name,
            name,
        } => {
            let result = store.rename_session(session_id, name.as_deref());
            return results
                .send(PersistenceResult::SessionRenamed {
                    request_id,
                    previous_name,
                    result,
                })
                .is_ok();
        }
        PersistenceRequest::DiscoverTransferSessions { current_session_id } => {
            let result = transfer::discover(store, current_session_id);
            return results
                .send(PersistenceResult::TransferSessions(result))
                .is_ok();
        }
        PersistenceRequest::TransferThought(request) => {
            let result = runtime
                .ok_or_else(|| "session transfer runtime is unavailable".to_owned())
                .and_then(|runtime| transfer::deliver(store, runtime, &request));
            return results
                .send(PersistenceResult::ThoughtTransferred { request, result })
                .is_ok();
        }
        PersistenceRequest::Lookup {
            request_id,
            identity,
        } => {
            let result = match identity {
                crate::ports::store::DurableIdentity::Operation(operation_id) => {
                    store.operation_request(operation_id)
                }
                crate::ports::store::DurableIdentity::Revision(revision_id) => {
                    store.revision_request(revision_id)
                }
            };
            return results
                .send(PersistenceResult::Lookup { request_id, result })
                .is_ok();
        }
        request @ (PersistenceRequest::PrepareSubmission(_)
        | PersistenceRequest::MarkSubmissionSending { .. }
        | PersistenceRequest::FinishSubmission { .. }) => {
            return process_submission(store, request, results);
        }
    };
    commit_batch(store, sequence, batch, retained, results, false)
}

fn process_submission(
    store: &mut SqliteStore,
    request: PersistenceRequest,
    results: &SyncSender<PersistenceResult>,
) -> bool {
    let outcome = match request {
        PersistenceRequest::PrepareSubmission(attempt) => {
            let submission_id = attempt.id;
            PersistenceResult::SubmissionPrepared {
                submission_id,
                result: store.prepare_submission(&attempt),
            }
        }
        PersistenceRequest::MarkSubmissionSending { submission_id, at } => {
            PersistenceResult::SubmissionSending {
                submission_id,
                result: store.mark_submission_sending(submission_id, at),
            }
        }
        PersistenceRequest::FinishSubmission {
            submission_id,
            outcome,
        } => PersistenceResult::SubmissionFinished {
            submission_id,
            result: store.finish_submission(submission_id, &outcome),
        },
        _ => return false,
    };
    results.send(outcome).is_ok()
}
fn retry_from(
    store: &mut SqliteStore,
    first: OperationSequence,
    retained: &mut BTreeMap<OperationSequence, Box<OperationBatch>>,
    results: &SyncSender<PersistenceResult>,
) -> bool {
    let sequences = retained
        .range(first..)
        .map(|(sequence, _)| *sequence)
        .collect::<Vec<_>>();
    if sequences.is_empty() {
        let result = Err(StoreError::NotFound(format!(
            "retained operation sequence {}",
            first.get()
        )));
        return results
            .send(PersistenceResult::Sequenced {
                sequence: first,
                result,
                retried: true,
            })
            .is_ok()
            && results.send(PersistenceResult::RetryFinished).is_ok();
    }
    for sequence in sequences {
        let Some(batch) = retained.get(&sequence).cloned() else {
            continue;
        };
        if !commit_batch(store, sequence, batch, retained, results, true) {
            return false;
        }
        if retained.contains_key(&sequence) {
            break;
        }
    }
    results.send(PersistenceResult::RetryFinished).is_ok()
}
fn commit_batch(
    store: &mut SqliteStore,
    sequence: OperationSequence,
    batch: Box<OperationBatch>,
    retained: &mut BTreeMap<OperationSequence, Box<OperationBatch>>,
    results: &SyncSender<PersistenceResult>,
    retried: bool,
) -> bool {
    let result = store.commit(&batch).and_then(|receipt| {
        receipt
            .ok_or_else(|| StoreError::Integrity("mutable operation lacked a receipt".to_owned()))
    });
    let result = if result.is_err() && !retention::can_retain(retained, sequence, &batch) {
        Err(StoreError::RecoveryCapacity)
    } else {
        result
    };
    if result.is_ok() {
        retained.remove(&sequence);
    } else if !matches!(result, Err(StoreError::RecoveryCapacity)) {
        retained.insert(sequence, batch);
    }
    results
        .send(PersistenceResult::Sequenced {
            sequence,
            result,
            retried,
        })
        .is_ok()
}

#[cfg(test)]
mod tests;
