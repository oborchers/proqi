//! Replay, conflict, and no-op orchestration for session administration.

#[path = "tests/concurrency.rs"]
mod concurrency;

use std::{cell::Cell, collections::HashMap, path::PathBuf, rc::Rc};

use crate::{
    application::{
        SessionServiceError,
        test_support::{TestClock, TestIds, TestRuntime},
    },
    domain::{BrowserOperationKind, OperationId, Session, SessionBoard, SessionId, Timestamp},
    ports::{
        environment::IdGenerator as _,
        runtime::RuntimeError,
        store::{
            BrowserCommitReceipt, BrowserHistoryStatus, CommitReceipt, FirstRunBoard,
            FirstRunOutcome, OperationBatch, SessionHit, SessionQuery, SessionRequest,
            SessionRequestReceipt, SessionSnapshot, Store, StoreError, StoredOperationRequest,
            StoredSessionRequest,
        },
    },
};

use super::super::SessionService;

/// A write that another process committed first.
enum RacingCommit {
    /// The racing process retained this request under the same identity.
    Owned(StoredSessionRequest),
    /// The racing change conflicts with state but owns no identity.
    Unowned,
}

#[derive(Default)]
struct AdministrationStore {
    racing: Option<RacingCommit>,
    held: Rc<Cell<usize>>,
    unleased_writes: usize,
    session: Option<Session>,
    requests: HashMap<OperationId, StoredSessionRequest>,
    history: BrowserHistoryStatus,
    noop_trashes: usize,
    browser_commits: usize,
    prunes: usize,
    history_moves: usize,
}

impl AdministrationStore {
    fn with_session(session: Session) -> Self {
        Self {
            session: Some(session),
            ..Self::default()
        }
    }

    fn write(&mut self, operation_id: OperationId) -> BrowserCommitReceipt {
        if self.held.get() == 0 {
            self.unleased_writes += 1;
        }
        Self::receipt(operation_id)
    }

    fn receipt(operation_id: OperationId) -> BrowserCommitReceipt {
        BrowserCommitReceipt {
            operation_id,
            cursor: 3,
            idempotent_replay: false,
        }
    }
}

impl Store for AdministrationStore {
    fn load_session(&mut self, id: SessionId) -> Result<SessionSnapshot, StoreError> {
        let session = self
            .session
            .clone()
            .filter(|session| session.id == id)
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

    fn search_sessions(&mut self, _query: &SessionQuery) -> Result<Vec<SessionHit>, StoreError> {
        Err(unused())
    }

    fn record_session_open(
        &mut self,
        _id: SessionId,
        _cwd: &std::path::Path,
        _at: Timestamp,
    ) -> Result<(), StoreError> {
        Err(unused())
    }

    fn rename_session(&mut self, _id: SessionId, _name: Option<&str>) -> Result<(), StoreError> {
        Err(unused())
    }

    fn commit_browser_operation(
        &mut self,
        operation: &crate::domain::BrowserOperation,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        if let Some(racing) = self.racing.take() {
            if let RacingCommit::Owned(owner) = racing {
                self.requests.insert(operation.id(), owner);
            }
            return Err(StoreError::Conflict("concurrent change".to_owned()));
        }
        self.browser_commits += 1;
        Ok(self.write(operation.id()))
    }

    fn browser_history_status(&mut self) -> Result<BrowserHistoryStatus, StoreError> {
        Ok(self.history)
    }

    fn move_browser_history(
        &mut self,
        operation_id: OperationId,
        _target: crate::ports::store::BrowserHistoryEntry,
        _undo: bool,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        self.history_moves += 1;
        Ok(self.write(operation_id))
    }

    fn session_request(
        &mut self,
        operation_id: OperationId,
    ) -> Result<Option<StoredSessionRequest>, StoreError> {
        Ok(self.requests.get(&operation_id).cloned())
    }

    fn commit_noop_trash(
        &mut self,
        operation_id: OperationId,
        _session_id: SessionId,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        self.noop_trashes += 1;
        Ok(self.write(operation_id))
    }

    fn prune_session_request(
        &mut self,
        _session_id: SessionId,
        operation_id: OperationId,
        _at: Timestamp,
    ) -> Result<BrowserCommitReceipt, StoreError> {
        self.prunes += 1;
        Ok(self.write(operation_id))
    }

    fn operation_request(
        &mut self,
        _id: OperationId,
    ) -> Result<Option<StoredOperationRequest>, StoreError> {
        Err(unused())
    }

    fn revision_request(
        &mut self,
        _id: crate::domain::RevisionId,
    ) -> Result<Option<StoredOperationRequest>, StoreError> {
        Err(unused())
    }

    fn create_first_run_session(
        &mut self,
        _board: &FirstRunBoard,
    ) -> Result<FirstRunOutcome, StoreError> {
        Err(unused())
    }

    fn commit(&mut self, _batch: &OperationBatch) -> Result<Option<CommitReceipt>, StoreError> {
        Err(unused())
    }

    fn trash_session(&mut self, _id: SessionId, _at: Timestamp) -> Result<(), StoreError> {
        Err(unused())
    }

    fn restore_session(&mut self, _id: SessionId) -> Result<(), StoreError> {
        Err(unused())
    }

    fn prune_session(&mut self, _id: SessionId) -> Result<(), StoreError> {
        Err(unused())
    }
}

fn unused() -> StoreError {
    StoreError::Invariant("unexpected store call in session administration test".to_owned())
}

fn session(ids: &mut TestIds, trashed: bool) -> Session {
    let mut session = Session::new(
        ids.session_id(),
        PathBuf::from("/work/session"),
        Timestamp::from_millis(10),
    )
    .expect("session");
    session.deleted_at = trashed.then_some(Timestamp::from_millis(20));
    session
}

fn stored(operation_id: OperationId, request: SessionRequest) -> StoredSessionRequest {
    StoredSessionRequest::Administration(SessionRequestReceipt {
        operation_id,
        request,
        cursor: 7,
        history_target: Some(BrowserOperationKind::Trash),
    })
}

#[test]
fn an_exact_replay_succeeds_before_the_busy_session_lease() {
    let mut ids = TestIds::new(1_725_300_000_000);
    let session = session(&mut ids, true);
    let id = session.id;
    let operation_id = ids.operation_id();
    let mut store = AdministrationStore::with_session(session);
    store.requests.insert(
        operation_id,
        stored(operation_id, SessionRequest::Trash { session_id: id }),
    );
    let runtime = TestRuntime { busy: Some(id) };
    let clock = TestClock(Timestamp::from_millis(30));
    let receipt = SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into())
        .expect("service")
        .trash_session(id, Some(operation_id))
        .expect("replay");

    assert!(receipt.idempotent_replay);
    assert!(!receipt.changed);
    assert_eq!(receipt.operation_id, operation_id);
    assert_eq!(store.noop_trashes + store.browser_commits, 0);
}

#[test]
fn an_identity_owned_by_another_request_conflicts_without_mutation() {
    let mut ids = TestIds::new(1_725_300_100_000);
    let session = session(&mut ids, false);
    let id = session.id;
    let renamed = ids.operation_id();
    let history = ids.operation_id();
    let mut store = AdministrationStore::with_session(session);
    store.requests.insert(
        renamed,
        stored(
            renamed,
            SessionRequest::Rename {
                session_id: id,
                name: Some("other".to_owned()),
            },
        ),
    );
    store
        .requests
        .insert(history, StoredSessionRequest::SessionHistory);
    let runtime = TestRuntime { busy: None };
    let clock = TestClock(Timestamp::from_millis(30));
    let mut service =
        SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into()).expect("service");

    for operation_id in [renamed, history] {
        assert!(matches!(
            service.trash_session(id, Some(operation_id)),
            Err(SessionServiceError::IdempotencyConflict)
        ));
    }
    assert!(matches!(
        service.rename_session(id, Some("changed"), Some(renamed)),
        Err(SessionServiceError::IdempotencyConflict)
    ));
    assert!(matches!(
        service.rename_session(id, Some("other"), Some(renamed)),
        Ok(receipt) if receipt.idempotent_replay
    ));
    drop(service);
    assert_eq!(store.browser_commits, 0);
}

#[test]
fn trashing_a_trashed_session_reserves_an_unchanged_receipt() {
    let mut ids = TestIds::new(1_725_300_200_000);
    let session = session(&mut ids, true);
    let id = session.id;
    let mut store = AdministrationStore::with_session(session);
    let runtime = TestRuntime { busy: None };
    let clock = TestClock(Timestamp::from_millis(30));
    let receipt = SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into())
        .expect("service")
        .trash_session(id, None)
        .expect("no-op trash");

    assert!(!receipt.changed);
    assert!(!receipt.idempotent_replay);
    assert_eq!(store.noop_trashes, 1);
    assert_eq!(store.browser_commits, 0);
}

#[test]
fn live_sessions_cannot_be_pruned_or_restored() {
    let mut ids = TestIds::new(1_725_300_300_000);
    let session = session(&mut ids, false);
    let id = session.id;
    let mut store = AdministrationStore::with_session(session);
    let runtime = TestRuntime { busy: None };
    let clock = TestClock(Timestamp::from_millis(30));
    let mut service =
        SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into()).expect("service");

    assert!(matches!(
        service.prune_session(id, None),
        Err(SessionServiceError::SessionNotTrashed(found)) if found == id
    ));
    assert!(matches!(
        service.restore_session(id, None),
        Err(SessionServiceError::SessionNotTrashed(found)) if found == id
    ));
    drop(service);
    assert_eq!(store.prunes + store.browser_commits, 0);
}

#[test]
fn a_history_replay_returns_its_retained_cursor_without_moving_history() {
    let mut ids = TestIds::new(1_725_300_400_000);
    let operation_id = ids.operation_id();
    let mut store = AdministrationStore::default();
    store.requests.insert(
        operation_id,
        stored(operation_id, SessionRequest::History { undo: true }),
    );
    let runtime = TestRuntime { busy: None };
    let clock = TestClock(Timestamp::from_millis(30));
    let mut service =
        SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into()).expect("service");

    let replay = service
        .move_browser_history(true, Some(operation_id))
        .expect("replay");
    assert!(replay.idempotent_replay);
    assert_eq!(replay.cursor, 7);
    assert_eq!(replay.target, Some(BrowserOperationKind::Trash));
    assert!(matches!(
        service.move_browser_history(false, Some(operation_id)),
        Err(SessionServiceError::IdempotencyConflict)
    ));
    drop(service);
    assert_eq!(store.history_moves, 0);
}

#[test]
fn blank_names_fail_before_the_lease_or_a_write() {
    let mut ids = TestIds::new(1_725_300_500_000);
    let session = session(&mut ids, false);
    let id = session.id;
    let mut store = AdministrationStore::with_session(session);
    let runtime = TestRuntime { busy: Some(id) };
    let clock = TestClock(Timestamp::from_millis(30));
    let result = SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into())
        .expect("service")
        .rename_session(id, Some("  "), None);

    assert!(matches!(result, Err(SessionServiceError::Domain(_))));
    assert!(!matches!(
        result,
        Err(SessionServiceError::Runtime(
            RuntimeError::SessionBusy { .. }
        ))
    ));
    assert_eq!(store.browser_commits, 0);
}
