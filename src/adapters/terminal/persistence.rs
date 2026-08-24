//! Ordered persistence lane with explicit retention for failed batches.

mod lane;
mod transfer;

use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    adapters::{runtime::FileRuntimeCoordinator, sqlite::SqliteStore},
    application::ThoughtMutation,
    domain::{OperationId, OperationSequence, RequestId, SessionId},
    ports::{
        store::{
            CommitReceipt, OperationBatch, SessionHit, Store, StoreError, StoredOperationRequest,
        },
        transfer::SessionTransferRequest,
    },
};

use super::TerminalError;

pub(super) enum PersistenceResult {
    Sequenced {
        sequence: OperationSequence,
        result: Result<CommitReceipt, StoreError>,
        retried: bool,
    },
    RetryFinished,
    Metadata {
        result: Result<(), StoreError>,
    },
    SessionRenamed {
        previous_name: Option<String>,
        result: Result<(), StoreError>,
    },
    TransferSessions(Result<Vec<SessionHit>, StoreError>),
    ThoughtTransferred {
        request: SessionTransferRequest,
        result: Result<ThoughtMutation, String>,
    },
    Lookup {
        request_id: RequestId,
        result: Result<Option<StoredOperationRequest>, StoreError>,
    },
}
enum PersistenceRequest {
    Commit(Box<OperationBatch>),
    Metadata(Box<OperationBatch>),
    RenameSession {
        session_id: SessionId,
        previous_name: Option<String>,
        name: Option<String>,
    },
    DiscoverTransferSessions {
        current_session_id: SessionId,
    },
    TransferThought(SessionTransferRequest),
    Retry(OperationSequence),
    Lookup {
        request_id: RequestId,
        operation_id: OperationId,
    },
}
pub(super) struct PersistenceLane {
    sender: Option<SyncSender<PersistenceRequest>>,
    pub(super) receiver: Receiver<PersistenceResult>,
    handle: Option<JoinHandle<()>>,
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
            session_id,
            previous_name,
            name,
        } => {
            let result = store.rename_session(session_id, name.as_deref());
            return results
                .send(PersistenceResult::SessionRenamed {
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
            operation_id,
        } => {
            let result = store.operation_request(operation_id);
            return results
                .send(PersistenceResult::Lookup { request_id, result })
                .is_ok();
        }
    };
    commit_batch(store, sequence, batch, retained, results, false)
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
    if result.is_ok() {
        retained.remove(&sequence);
    } else {
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
mod tests {
    use std::time::Duration;

    use crate::{
        adapters::{
            memory::FakeIdGenerator,
            sqlite::{RetryPolicy, StoreConfig},
        },
        application::{Action, AppState, Effect, reduce},
        domain::{OperationSequence, Session, SessionBoard, Timestamp},
        ports::{
            environment::IdGenerator,
            store::{CommitReceipt, MigrationMode, OperationBatch, Store, StoreError},
        },
    };

    use super::{PersistenceLane, PersistenceResult};

    fn assert_ordered_results(lane: &PersistenceLane, succeed: bool) {
        for expected in [OperationSequence::new(1), OperationSequence::new(2)] {
            let result = lane
                .receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("persistence result");
            let (sequence, result) = sequenced(result);
            assert_eq!(sequence, expected);
            assert_eq!(result.is_ok(), succeed);
        }
    }

    #[test]
    fn failed_batch_is_retained_and_retried_after_contention() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = directory.path().join("proqi.sqlite3");
        let mut config = StoreConfig::new(
            database.clone(),
            directory.path().join("backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(1),
        );
        config.retry = RetryPolicy {
            busy_timeout: Duration::from_millis(1),
            max_attempts: 1,
            base_delay: Duration::ZERO,
            jitter_seed: 1,
        };
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-persistence-lane"),
            Timestamp::from_millis(2),
        )
        .expect("session");
        let mut setup = crate::adapters::sqlite::SqliteStore::open(&config).expect("store");
        setup
            .commit(&OperationBatch::CreateSession(session.clone()))
            .expect("create session");
        let lane_store = crate::adapters::sqlite::SqliteStore::open(&config).expect("lane store");
        let lane = PersistenceLane::spawn(lane_store);
        let mut state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
        let effects = reduce(
            &mut state,
            Action::CreateThought {
                thought_id: ids.thought_id(),
                operation_id: ids.operation_id(),
                content: "retained through contention".to_owned(),
                annotations: Vec::new(),
                insertion_index: None,
                at: Timestamp::from_millis(3),
            },
        )
        .expect("create thought");
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("expected board operation");
        };
        let sequence = operation.sequence;
        let lock = setup.acquire_test_write_lock().expect("acquire writer");
        lane.commit(OperationBatch::Board(operation.clone()))
            .expect("queue commit");
        let failed = lane
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("failure result");
        let (failed_sequence, failed_result) = sequenced(failed);
        assert_eq!(failed_sequence, sequence);
        assert!(failed_result.is_err());
        lock.release().expect("release writer");

        lane.retry(sequence).expect("queue retry");
        let retried = lane
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("retry result");
        let (retried_sequence, retried_result) = sequenced(retried);
        assert_eq!(retried_sequence, sequence);
        assert!(retried_result.is_ok());
        assert!(matches!(
            lane.receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("retry completion"),
            PersistenceResult::RetryFinished
        ));
        lane.stop().expect("stop lane");
        let snapshot = setup
            .load_session(state.board.session.id)
            .expect("snapshot");
        assert_eq!(
            snapshot.board.live_thoughts()[0].content,
            "retained through contention"
        );
    }

    #[test]
    fn retry_replays_every_retained_batch_in_sequence_order() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let mut config = StoreConfig::new(
            directory.path().join("proqi.sqlite3"),
            directory.path().join("backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(1),
        );
        config.retry = RetryPolicy {
            busy_timeout: Duration::from_millis(1),
            max_attempts: 1,
            base_delay: Duration::ZERO,
            jitter_seed: 1,
        };
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-persistence-order"),
            Timestamp::from_millis(2),
        )
        .expect("session");
        let mut setup = crate::adapters::sqlite::SqliteStore::open(&config).expect("store");
        setup
            .commit(&OperationBatch::CreateSession(session.clone()))
            .expect("create session");
        let lane_store = crate::adapters::sqlite::SqliteStore::open(&config).expect("lane store");
        let lane = PersistenceLane::spawn(lane_store);
        let mut state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
        let mut batches = Vec::new();
        for content in ["first", "second"] {
            let effects = reduce(
                &mut state,
                Action::CreateThought {
                    thought_id: ids.thought_id(),
                    operation_id: ids.operation_id(),
                    content: content.to_owned(),
                    annotations: Vec::new(),
                    insertion_index: None,
                    at: Timestamp::from_millis(3),
                },
            )
            .expect("create thought");
            let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
                panic!("expected board operation");
            };
            batches.push(OperationBatch::Board(operation.clone()));
        }
        let lock = setup.acquire_test_write_lock().expect("acquire writer");
        for batch in batches {
            lane.commit(batch).expect("queue commit");
        }
        assert_ordered_results(&lane, false);
        lock.release().expect("release writer");
        lane.retry(OperationSequence::new(1)).expect("queue retry");
        assert_ordered_results(&lane, true);
        assert!(matches!(
            lane.receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("retry completion"),
            PersistenceResult::RetryFinished
        ));
        lane.stop().expect("stop lane");
        let snapshot = setup
            .load_session(state.board.session.id)
            .expect("snapshot");
        assert_eq!(snapshot.board.live_thoughts().len(), 2);
    }

    #[test]
    fn integration_metadata_commits_without_a_sequence_receipt() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database = directory.path().join("proqi.sqlite3");
        let config = StoreConfig::new(
            database,
            directory.path().join("backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(1),
        );
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-metadata-lane"),
            Timestamp::from_millis(2),
        )
        .expect("session");
        let mut setup = crate::adapters::sqlite::SqliteStore::open(&config).expect("store");
        setup
            .commit(&OperationBatch::CreateSession(session.clone()))
            .expect("create session");
        let lane_store = crate::adapters::sqlite::SqliteStore::open(&config).expect("lane store");
        let lane = PersistenceLane::spawn(lane_store);
        let context = crate::domain::IntegrationContext {
            provider: "herdr".to_owned(),
            direction: crate::domain::Direction::Right,
            agent_kind: "codex".to_owned(),
            agent_name: "fixture".to_owned(),
            workspace_hint: Some("w1".to_owned()),
            tab_hint: Some("w1:t1".to_owned()),
            pane_hint: Some("w1:p2".to_owned()),
            verified_at: Timestamp::from_millis(3),
        };
        lane.metadata(OperationBatch::IntegrationContext {
            session_id: session.id,
            context: Some(context.clone()),
        })
        .expect("queue metadata");
        let outcome = lane
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("metadata result");
        assert!(matches!(
            outcome,
            PersistenceResult::Metadata { result: Ok(()) }
        ));
        lane.stop().expect("stop lane");
        assert_eq!(
            setup
                .load_session(session.id)
                .expect("snapshot")
                .integration_context,
            Some(context)
        );
    }

    fn sequenced(
        outcome: PersistenceResult,
    ) -> (OperationSequence, Result<CommitReceipt, StoreError>) {
        let PersistenceResult::Sequenced {
            sequence, result, ..
        } = outcome
        else {
            panic!("expected sequenced persistence result");
        };
        (sequence, result)
    }
}
