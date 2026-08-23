//! Ordered persistence lane with explicit retention for failed batches.

use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    adapters::sqlite::SqliteStore,
    domain::OperationSequence,
    ports::store::{OperationBatch, Store, StoreError},
};

use super::TerminalError;

pub(super) struct PersistenceResult {
    pub(super) sequence: OperationSequence,
    pub(super) result: Result<(), StoreError>,
}

enum PersistenceRequest {
    Commit(Box<OperationBatch>),
    Retry(OperationSequence),
}

pub(super) struct PersistenceLane {
    sender: Option<SyncSender<PersistenceRequest>>,
    pub(super) receiver: Receiver<PersistenceResult>,
    handle: Option<JoinHandle<()>>,
}

impl PersistenceLane {
    pub(super) fn spawn(store: SqliteStore) -> Self {
        let (request_sender, request_receiver) = sync_channel(64);
        let (result_sender, result_receiver) = sync_channel(64);
        let handle =
            thread::spawn(move || persistence_loop(store, &request_receiver, &result_sender));
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
        }
    }

    pub(super) fn commit(&self, batch: OperationBatch) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::Commit(Box::new(batch)))
    }

    pub(super) fn retry(&self, sequence: OperationSequence) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::Retry(sequence))
    }

    fn send(&self, request: PersistenceRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("persistence lane is closed"))?
            .send(request)
            .map_err(|_| TerminalError::Worker("persistence lane disconnected"))
    }

    pub(super) fn stop(mut self) -> Result<(), TerminalError> {
        drop(self.sender.take());
        match self.handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(TerminalError::Worker("persistence lane panicked")),
        }
    }
}

fn persistence_loop(
    mut store: SqliteStore,
    requests: &Receiver<PersistenceRequest>,
    results: &SyncSender<PersistenceResult>,
) {
    let mut retained = BTreeMap::new();
    while let Ok(request) = requests.recv() {
        if !process_request(&mut store, request, &mut retained, results) {
            return;
        }
    }
}

fn process_request(
    store: &mut SqliteStore,
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
            let Some(batch) = retained.get(&sequence).cloned() else {
                let result = Err(StoreError::NotFound(format!(
                    "retained operation sequence {}",
                    sequence.get()
                )));
                return results.send(PersistenceResult { sequence, result }).is_ok();
            };
            (sequence, batch)
        }
    };
    let result = store.commit(&batch).map(|_receipt| ());
    if result.is_ok() {
        retained.remove(&sequence);
    } else {
        retained.insert(sequence, batch);
    }
    results.send(PersistenceResult { sequence, result }).is_ok()
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use crate::{
        adapters::{
            memory::FakeIdGenerator,
            sqlite::{RetryPolicy, StoreConfig},
        },
        application::{Action, AppState, Effect, reduce},
        domain::{Session, SessionBoard, Timestamp},
        ports::{
            environment::IdGenerator,
            store::{MigrationMode, OperationBatch, Store},
        },
    };

    use super::PersistenceLane;

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
            PathBuf::from("/tmp/proqi-persistence-lane"),
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
        assert_eq!(failed.sequence, sequence);
        assert!(failed.result.is_err());
        lock.release().expect("release writer");

        lane.retry(sequence).expect("queue retry");
        let retried = lane
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("retry result");
        assert_eq!(retried.sequence, sequence);
        assert!(retried.result.is_ok());
        lane.stop().expect("stop lane");
        let snapshot = setup
            .load_session(state.board.session.id)
            .expect("snapshot");
        assert_eq!(
            snapshot.board.live_thoughts()[0].content,
            "retained through contention"
        );
    }
}
