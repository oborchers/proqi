//! Real cache locks serialize refresh/adoption; bounded busy is not corruption.

use std::{fs, thread};

use crate::{
    domain::{StableVersion, Timestamp},
    ports::update::{
        ExternalCacheTransition, ReleaseObservation, UpdateError, UpdateStateStore as _,
    },
};

use super::super::{
    FileUpdateStateStore, lock_state,
    test_sync::{Rendezvous, Stage},
};
use super::{external::pending, identity};

#[derive(Clone, Copy)]
enum Mutation {
    Reconcile,
    Refresh,
}

impl Mutation {
    fn apply(self, store: &FileUpdateStateStore) -> Result<(), UpdateError> {
        let old = StableVersion::parse("0.9.0").expect("old");
        let current = StableVersion::parse("0.10.0").expect("current");
        match self {
            Self::Reconcile => {
                let result = store.reconcile_external_upgrade(
                    identity(),
                    &old,
                    &current,
                    Some(&pending(&current)),
                )?;
                assert!(matches!(
                    result,
                    ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied
                ));
            }
            Self::Refresh => {
                store.record_success(
                    identity(),
                    ReleaseObservation::Latest {
                        version: current.clone(),
                        etag: Some("race".to_owned()),
                    },
                    current,
                    Timestamp::from_millis(5),
                )?;
            }
        }
        Ok(())
    }
}

#[test]
fn concurrent_release_refresh_and_external_reconcile_preserve_pending_authority() {
    for (first, second) in [
        (Mutation::Reconcile, Mutation::Refresh),
        (Mutation::Refresh, Mutation::Reconcile),
    ] {
        let temporary = tempfile::tempdir().expect("cache root");
        let store = initial_store(temporary.path());
        let mut first_store = store.clone();
        let acquired = Rendezvous::install(&mut first_store, Stage::Acquired);
        let mut second_store = store.clone();
        let contended = Rendezvous::install(&mut second_store, Stage::Contended);
        thread::scope(|scope| {
            let first_worker = scope.spawn(move || first.apply(&first_store));
            acquired.reached();
            let second_worker = scope.spawn(move || second.apply(&second_store));
            contended.reached(); // The second OS lock really returned WouldBlock.
            acquired.release();
            first_worker
                .join()
                .expect("first writer")
                .expect("first mutation");
            contended.release(); // Resume only after the first durable write released its lease.
            second_worker
                .join()
                .expect("second writer")
                .expect("second mutation");
        });
        assert_authority(&store);
    }
}

#[test]
fn exhausted_state_lock_changes_no_bytes_and_exact_retry_preserves_authority() {
    for mutation in [Mutation::Refresh, Mutation::Reconcile] {
        let temporary = tempfile::tempdir().expect("cache root");
        let store = initial_store(temporary.path());
        Mutation::Reconcile.apply(&store).expect("pending adoption");
        Mutation::Refresh
            .apply(&store)
            .expect("existing release evidence");
        let directory = store
            .installation_dir(identity())
            .expect("installation root");
        let before = fs::read(directory.join("state.json")).expect("durable state");
        let lease = lock_state(&directory.join("state.lock"), None).expect("held state lock");
        assert_eq!(
            mutation.apply(&store),
            Err(UpdateError::State(
                "update state lock remained busy".to_owned()
            ))
        );
        assert_eq!(
            fs::read(directory.join("state.json")).expect("unchanged state"),
            before
        );
        drop(lease);
        mutation.apply(&store).expect("exact retry after release");
        assert_authority(&store);
    }
}

fn initial_store(root: &std::path::Path) -> FileUpdateStateStore {
    let store = FileUpdateStateStore::new(root).expect("store");
    store
        .record_restart_state(
            identity(),
            StableVersion::parse("0.9.0").expect("old"),
            true,
        )
        .expect("stale observation");
    store
}

fn assert_authority(store: &FileUpdateStateStore) {
    let current = StableVersion::parse("0.10.0").expect("current");
    let state = store.load(identity()).expect("final state");
    assert_eq!(state.observed_installed_version, Some(current.clone()));
    assert!(state.restart_needed);
    assert_eq!(state.external_restart, Some(pending(&current)));
    assert_eq!(state.latest_stable, Some(current));
}
