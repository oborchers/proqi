//! Fresh and resumed session load policy.

use std::path::{Path, PathBuf};

use crate::{
    application::test_support::{TestClock, TestIds},
    domain::{OperationId, RevisionId, Session, SessionBoard, SessionId, Timestamp},
    ports::{
        runtime::{Lease, RuntimeCoordinator, RuntimeError, RuntimeScan},
        store::{
            CommitReceipt, FirstRunBoard, FirstRunOutcome, OperationBatch, SessionHit,
            SessionQuery, SessionSnapshot, Store, StoreError, StoredOperationRequest,
        },
    },
};

use super::{SessionService, SessionServiceError};

#[derive(Default)]
struct BusyCompactionStore {
    session: Option<Session>,
    compact_calls: usize,
}

impl Store for BusyCompactionStore {
    fn load_session(&mut self, id: SessionId) -> Result<SessionSnapshot, StoreError> {
        let session = self
            .session
            .as_ref()
            .filter(|session| session.id == id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        let board = SessionBoard::new(session, Vec::new())
            .map_err(|error| StoreError::Invariant(error.to_string()))?;
        Ok(SessionSnapshot {
            board,
            board_operations: Vec::new(),
            board_history_cursor: 0,
            revisions: Vec::new(),
            editor_history_cursors: Vec::new(),
            integration_context: None,
        })
    }

    fn compact_session(&mut self, _id: SessionId) -> Result<(), StoreError> {
        self.compact_calls += 1;
        Err(StoreError::Busy)
    }

    fn search_sessions(&mut self, _query: &SessionQuery) -> Result<Vec<SessionHit>, StoreError> {
        Err(unused_store_call())
    }

    fn record_session_open(
        &mut self,
        _id: SessionId,
        _cwd: &Path,
        _at: Timestamp,
    ) -> Result<(), StoreError> {
        Err(unused_store_call())
    }

    fn rename_session(&mut self, _id: SessionId, _name: Option<&str>) -> Result<(), StoreError> {
        Err(unused_store_call())
    }

    fn operation_request(
        &mut self,
        _id: OperationId,
    ) -> Result<Option<StoredOperationRequest>, StoreError> {
        Err(unused_store_call())
    }

    fn revision_request(
        &mut self,
        _id: RevisionId,
    ) -> Result<Option<StoredOperationRequest>, StoreError> {
        Err(unused_store_call())
    }

    fn create_first_run_session(
        &mut self,
        _board: &FirstRunBoard,
    ) -> Result<FirstRunOutcome, StoreError> {
        Err(unused_store_call())
    }

    fn commit(&mut self, batch: &OperationBatch) -> Result<Option<CommitReceipt>, StoreError> {
        let OperationBatch::CreateSession(session) = batch else {
            return Err(unused_store_call());
        };
        self.session = Some(session.clone());
        Ok(None)
    }

    fn trash_session(&mut self, _id: SessionId, _at: Timestamp) -> Result<(), StoreError> {
        Err(unused_store_call())
    }

    fn restore_session(&mut self, _id: SessionId) -> Result<(), StoreError> {
        Err(unused_store_call())
    }

    fn prune_session(&mut self, _id: SessionId) -> Result<(), StoreError> {
        Err(unused_store_call())
    }
}

fn unused_store_call() -> StoreError {
    StoreError::Invariant("unexpected store call in session load policy test".to_owned())
}

#[derive(Clone, Copy)]
struct TestLease;

impl Lease for TestLease {}

struct TestRuntime;

impl RuntimeCoordinator for TestRuntime {
    type SessionLease = TestLease;
    type SharedSchemaLease = TestLease;
    type ExclusiveSchemaLease = TestLease;

    fn acquire_session(&self, _session_id: SessionId) -> Result<TestLease, RuntimeError> {
        Ok(TestLease)
    }

    fn acquire_schema_shared(&self) -> Result<TestLease, RuntimeError> {
        Ok(TestLease)
    }

    fn acquire_schema_exclusive(&self) -> Result<TestLease, RuntimeError> {
        Ok(TestLease)
    }

    fn scan_runtime(&self) -> Result<RuntimeScan, RuntimeError> {
        Ok(RuntimeScan::default())
    }
}

fn test_directory() -> PathBuf {
    std::env::temp_dir().join("proqi-session-load-policy")
}

#[test]
fn fresh_session_load_skips_unnecessary_compaction() {
    let mut store = BusyCompactionStore::default();
    let runtime = TestRuntime;
    let clock = TestClock(Timestamp::from_millis(1));
    let mut ids = TestIds::new(1_725_000_000_000);
    let session = SessionService::new(&mut store, &runtime, &clock, &mut ids, test_directory())
        .expect("service")
        .create_session()
        .expect("fresh session");

    assert_eq!(store.compact_calls, 0);
    assert_eq!(session.state.board.live_thoughts().len(), 0);
}

#[test]
fn resumed_session_retains_history_compaction() {
    let mut store = BusyCompactionStore::default();
    let runtime = TestRuntime;
    let clock = TestClock(Timestamp::from_millis(1));
    let mut ids = TestIds::new(1_725_000_000_000);
    let session_id = {
        let mut service =
            SessionService::new(&mut store, &runtime, &clock, &mut ids, test_directory())
                .expect("service");
        service
            .create_session()
            .expect("fresh session")
            .state
            .board
            .session
            .id
    };
    let resumed = SessionService::new(&mut store, &runtime, &clock, &mut ids, test_directory())
        .expect("service")
        .resume(session_id);

    assert!(matches!(
        resumed,
        Err(SessionServiceError::Store(StoreError::Busy))
    ));
    assert_eq!(store.compact_calls, 1);
}
