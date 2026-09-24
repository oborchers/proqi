//! Lease lifetime, identity races, and forwarded-rename admission.

use std::{cell::Cell, rc::Rc};

use super::{AdministrationStore, RacingCommit, session, stored};
use crate::{
    application::{
        RenameAdmission, SessionService, SessionServiceError,
        test_support::{TestClock, TestIds, TestRuntime},
    },
    domain::{SessionId, Timestamp},
    ports::{
        environment::IdGenerator as _,
        runtime::{Lease, RuntimeCoordinator, RuntimeError, RuntimeScan},
        store::SessionRequest,
    },
};

struct TrackedLease(Rc<Cell<usize>>);

impl Lease for TrackedLease {}

impl Drop for TrackedLease {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

/// Runtime whose leases report whether any is still held.
struct TrackingRuntime(Rc<Cell<usize>>);

impl RuntimeCoordinator for TrackingRuntime {
    type SessionLease = TrackedLease;
    type SharedSchemaLease = TrackedLease;
    type ExclusiveSchemaLease = TrackedLease;

    fn acquire_session(&self, _session_id: SessionId) -> Result<TrackedLease, RuntimeError> {
        self.0.set(self.0.get() + 1);
        Ok(TrackedLease(Rc::clone(&self.0)))
    }

    fn acquire_schema_shared(&self) -> Result<TrackedLease, RuntimeError> {
        self.acquire_session(
            SessionId::from_database_bytes([0; 16])
                .map_err(|_| RuntimeError::Invalid("test lease".to_owned()))?,
        )
    }

    fn acquire_schema_exclusive(&self) -> Result<TrackedLease, RuntimeError> {
        self.acquire_schema_shared()
    }

    fn scan_runtime(&self) -> Result<RuntimeScan, RuntimeError> {
        Ok(RuntimeScan::default())
    }
}

#[test]
fn every_administration_write_happens_while_the_session_lease_is_held() {
    let mut ids = TestIds::new(1_725_300_600_000);
    let live = session(&mut ids, false);
    let id = live.id;
    let mut store = AdministrationStore::with_session(live);
    let runtime = TrackingRuntime(Rc::clone(&store.held));
    let clock = TestClock(Timestamp::from_millis(30));
    let mut service =
        SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into()).expect("service");
    service
        .rename_session(id, Some("renamed"), None)
        .expect("rename");
    service.trash_session(id, None).expect("trash");
    drop(service);
    store.session = store.session.take().map(|mut session| {
        session.deleted_at = Some(Timestamp::from_millis(31));
        session
    });
    let mut service =
        SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into()).expect("service");
    service.trash_session(id, None).expect("no-op trash");
    service.restore_session(id, None).expect("restore");
    service.prune_session(id, None).expect("prune");
    drop(service);

    assert_eq!(store.browser_commits + store.noop_trashes + store.prunes, 5);
    assert_eq!(
        store.unleased_writes, 0,
        "a write ran after its lease was dropped"
    );
    assert_eq!(
        store.held.get(),
        0,
        "every lease is released after its request"
    );
}

#[test]
fn a_store_conflict_from_a_concurrent_identity_is_resolved_against_its_owner() {
    let mut ids = TestIds::new(1_725_300_700_000);
    let live = session(&mut ids, false);
    let id = live.id;
    let same = ids.operation_id();
    let other = ids.operation_id();
    let unused = ids.operation_id();
    let mut store = AdministrationStore::with_session(live);
    let runtime = TestRuntime { busy: None };
    let clock = TestClock(Timestamp::from_millis(30));

    store.racing = Some(RacingCommit::Owned(stored(
        same,
        SessionRequest::Trash { session_id: id },
    )));
    let replay = SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into())
        .expect("service")
        .trash_session(id, Some(same))
        .expect("the concurrent same request replays");
    assert!(replay.idempotent_replay);
    assert!(!replay.changed);

    let rename = SessionRequest::Rename {
        session_id: id,
        name: None,
    };
    store.racing = Some(RacingCommit::Owned(stored(other, rename)));
    let reused = SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into())
        .expect("service")
        .trash_session(id, Some(other));
    assert!(matches!(
        reused,
        Err(SessionServiceError::IdempotencyConflict)
    ));

    store.racing = Some(RacingCommit::Unowned);
    let state_conflict = SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into())
        .expect("service")
        .trash_session(id, Some(unused));
    assert!(matches!(
        state_conflict,
        Err(SessionServiceError::Store(
            crate::ports::store::StoreError::Conflict(_)
        ))
    ));
    assert_eq!(store.browser_commits, 0);
}

#[test]
fn rename_admission_replays_or_reports_the_change_it_will_forward() {
    let mut ids = TestIds::new(1_725_300_800_000);
    let mut live = session(&mut ids, false);
    live.name = Some("before".to_owned());
    let id = live.id;
    let applied = ids.operation_id();
    let mut store = AdministrationStore::with_session(live);
    store.requests.insert(
        applied,
        stored(
            applied,
            SessionRequest::Rename {
                session_id: id,
                name: Some("after".to_owned()),
            },
        ),
    );
    let runtime = TestRuntime { busy: Some(id) };
    let clock = TestClock(Timestamp::from_millis(30));
    let mut service =
        SessionService::new(&mut store, &runtime, &clock, &mut ids, "/".into()).expect("service");

    assert!(matches!(
        service.admit_rename(id, Some("after"), Some(applied)),
        Ok(RenameAdmission::Replayed(receipt)) if receipt.idempotent_replay
    ));
    assert!(matches!(
        service.admit_rename(id, Some("other"), Some(applied)),
        Err(SessionServiceError::IdempotencyConflict)
    ));
    assert!(matches!(
        service.admit_rename(id, Some("before"), None),
        Ok(RenameAdmission::New(receipt)) if !receipt.changed
    ));
    assert!(matches!(
        service.admit_rename(id, None, None),
        Ok(RenameAdmission::New(receipt)) if receipt.changed
    ));
    assert!(matches!(
        service.admit_rename(id, Some(" "), None),
        Err(SessionServiceError::Domain(_))
    ));
}
