use super::*;

pub(super) fn pending(target: &StableVersion) -> crate::domain::ExternalRestartPending {
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{ExternalRestartExpectation, ExternalRestartPending},
        ports::environment::IdGenerator as _,
    };

    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    ExternalRestartPending::new(
        target.clone(),
        ids.request_id(),
        vec![ExternalRestartExpectation::new(
            ids.session_id(),
            ids.instance_id(),
            42,
            StableVersion::parse("0.9.0").expect("old version"),
        )],
    )
    .expect("pending external restart")
}

#[test]
fn convergence_owner_excludes_startup_until_release() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let convergence = store
        .try_lock(identity(), UpdateLockKind::Convergence)
        .expect("convergence lock")
        .expect("convergence owner");
    assert!(
        store
            .try_startup_lock(identity())
            .expect("excluded follower")
            .is_none()
    );
    assert!(
        store
            .try_lock(identity(), UpdateLockKind::Convergence)
            .expect("exclusive contender")
            .is_none()
    );
    drop(convergence);
    let admission = store
        .try_startup_lock(identity())
        .expect("startup lock")
        .expect("shared admission after convergence release");
    let follower = store
        .try_startup_lock(identity())
        .expect("follower lock")
        .expect("shared follower");
    assert!(
        store
            .try_lock(identity(), UpdateLockKind::Convergence)
            .expect("shared admissions exclude convergence")
            .is_none()
    );
    drop((admission, follower));
    assert!(
        store
            .try_lock(identity(), UpdateLockKind::Convergence)
            .expect("released startup admissions")
            .is_some()
    );
}

#[test]
fn external_reconciliation_is_ordered_exact_and_idempotent() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let observed = StableVersion::parse("0.9.9").expect("observed");
    let installed = StableVersion::parse("0.10.0").expect("installed");
    store
        .record_restart_state(identity(), observed.clone(), true)
        .expect("initial state");
    assert_eq!(
        store
            .reconcile_external_upgrade(identity(), &observed, &installed, None)
            .expect("external reconcile"),
        ExternalCacheTransition::Applied
    );
    assert_eq!(
        store
            .reconcile_external_upgrade(identity(), &observed, &installed, None)
            .expect("idempotent reconcile"),
        ExternalCacheTransition::AlreadyApplied
    );
    assert_eq!(
        store
            .reconcile_external_upgrade(
                identity(),
                &installed,
                &StableVersion::parse("0.2.0").expect("lexically misleading older version"),
                None,
            )
            .expect("ordered conflict"),
        ExternalCacheTransition::Conflict
    );
    let state = store.load(identity()).expect("unchanged reconciled state");
    assert_eq!(state.observed_installed_version, Some(installed));
    assert!(!state.restart_needed);
}

#[test]
fn external_restart_completion_requires_the_exact_installed_version() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let installed = StableVersion::parse("0.10.0").expect("installed");
    let observed = StableVersion::parse("0.9.0").expect("observed");
    let pending = pending(&installed);
    store
        .record_restart_state(identity(), observed.clone(), true)
        .expect("pending restart");
    assert_eq!(
        store
            .reconcile_external_upgrade(identity(), &observed, &installed, Some(&pending))
            .expect("record exact pending restart"),
        ExternalCacheTransition::Applied
    );
    assert_eq!(
        store
            .complete_external_restart(
                identity(),
                &StableVersion::parse("0.10.1").expect("mismatch"),
                &pending,
            )
            .expect("mismatched completion"),
        ExternalCacheTransition::Conflict
    );
    assert!(
        store
            .load(identity())
            .expect("pending state")
            .restart_needed
    );
    assert_eq!(
        store
            .complete_external_restart(identity(), &installed, &pending)
            .expect("exact completion"),
        ExternalCacheTransition::Applied
    );
    assert_eq!(
        store
            .complete_external_restart(identity(), &installed, &pending)
            .expect("idempotent completion"),
        ExternalCacheTransition::AlreadyApplied
    );
}

#[test]
fn manual_resume_acknowledges_only_one_exact_pending_session_at_a_time() {
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{ExternalRestartExpectation, ExternalRestartPending},
        ports::environment::IdGenerator as _,
    };

    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let observed = StableVersion::parse("0.9.0").expect("observed");
    let installed = StableVersion::parse("0.10.0").expect("installed");
    let mut ids = FakeIdGenerator::new(1_800_010_000_000);
    let first = ids.session_id();
    let second = ids.session_id();
    let pending = ExternalRestartPending::new(
        installed.clone(),
        ids.request_id(),
        [first, second]
            .into_iter()
            .map(|session| {
                ExternalRestartExpectation::new(session, ids.instance_id(), 42, observed.clone())
            })
            .collect(),
    )
    .expect("pending cohort");
    store
        .record_restart_state(identity(), observed.clone(), true)
        .expect("stale observation");
    store
        .reconcile_external_upgrade(identity(), &observed, &installed, Some(&pending))
        .expect("record pending cohort");

    assert_eq!(
        store
            .acknowledge_external_resume(identity(), &installed, &pending, first)
            .expect("first manual resume"),
        ExternalCacheTransition::Applied
    );
    let partial = store.load(identity()).expect("partial pending cohort");
    let remaining = partial.external_restart.expect("remaining cohort");
    assert!(partial.restart_needed);
    assert_eq!(remaining.expectations().len(), 1);
    assert_eq!(remaining.expectations()[0].session_id(), second);
    assert!(remaining.manually_acknowledged(first));
    assert_eq!(
        store
            .acknowledge_external_resume(identity(), &installed, &pending, second)
            .expect("stale cohort cannot erase remaining state"),
        ExternalCacheTransition::Conflict
    );
    assert_eq!(
        store
            .acknowledge_external_resume(identity(), &installed, &remaining, second)
            .expect("final manual resume"),
        ExternalCacheTransition::Applied
    );
    let complete = store.load(identity()).expect("completed cohort");
    assert!(!complete.restart_needed);
    assert!(complete.external_restart.is_none());
}

#[test]
fn manual_resume_retry_recognizes_the_exact_state_committed_before_a_late_error() {
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{ExternalRestartExpectation, ExternalRestartPending},
        ports::environment::IdGenerator as _,
    };

    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let observed = StableVersion::parse("0.9.0").expect("observed");
    let installed = StableVersion::parse("0.10.0").expect("installed");
    let mut ids = FakeIdGenerator::new(1_800_020_000_000);
    let first = ids.session_id();
    let second = ids.session_id();
    let pending = ExternalRestartPending::new(
        installed.clone(),
        ids.request_id(),
        [first, second]
            .into_iter()
            .map(|session_id| {
                ExternalRestartExpectation::new(session_id, ids.instance_id(), 42, observed.clone())
            })
            .collect(),
    )
    .expect("pending cohort");
    store
        .record_restart_state(identity(), observed.clone(), true)
        .expect("stale observation");
    store
        .reconcile_external_upgrade(identity(), &observed, &installed, Some(&pending))
        .expect("record pending cohort");

    store.fail_next_write_after_rename();
    store
        .acknowledge_external_resume(identity(), &installed, &pending, first)
        .expect_err("post-rename durability failure is ambiguous");

    let fresh = FileUpdateStateStore::new(temporary.path()).expect("fresh process store");
    let committed = fresh.load(identity()).expect("committed post-rename state");
    let remaining = committed.external_restart.expect("unfinished peer");
    assert!(committed.restart_needed);
    assert_eq!(remaining.expectations()[0].session_id(), second);
    assert!(remaining.manually_acknowledged(first));
    assert_eq!(
        fresh
            .acknowledge_external_resume(identity(), &installed, &pending, first)
            .expect("fresh exact retry"),
        ExternalCacheTransition::AlreadyApplied
    );
}

#[test]
fn release_refresh_never_replaces_an_authoritative_installation_observation() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let old = StableVersion::parse("0.9.0").expect("old");
    let current = StableVersion::parse("0.10.0").expect("current");
    let observation = || ReleaseObservation::Latest {
        version: current.clone(),
        etag: None,
    };
    store
        .record_success(
            identity(),
            observation(),
            current.clone(),
            Timestamp::from_millis(1),
        )
        .expect("initial current observation");
    let obsolete = store
        .record_success(
            identity(),
            observation(),
            old.clone(),
            Timestamp::from_millis(2),
        )
        .expect("obsolete release refresh");
    assert_eq!(obsolete.observed_installed_version, Some(current.clone()));

    let other = InstallationIdentity::from_digest([20; 32]);
    store
        .record_success(other, observation(), old.clone(), Timestamp::from_millis(3))
        .expect("initial old observation");
    let newer = store
        .record_success(other, observation(), current, Timestamp::from_millis(4))
        .expect("newer release refresh");
    assert_eq!(newer.observed_installed_version, Some(old));
}

#[test]
fn concurrent_release_refresh_and_external_reconcile_preserve_pending_authority() {
    let temporary = tempfile::tempdir().expect("cache root");
    let store = FileUpdateStateStore::new(temporary.path()).expect("store");
    let old = StableVersion::parse("0.9.0").expect("old");
    let current = StableVersion::parse("0.10.0").expect("current");
    let pending = pending(&current);
    store
        .record_restart_state(identity(), old.clone(), true)
        .expect("stale observation");
    let barrier = Arc::new(Barrier::new(3));
    let reconcile_store = store.clone();
    let reconcile_barrier = Arc::clone(&barrier);
    let reconcile_old = old.clone();
    let reconcile_current = current.clone();
    let reconcile_pending = pending.clone();
    let reconcile = std::thread::spawn(move || {
        reconcile_barrier.wait();
        reconcile_store.reconcile_external_upgrade(
            identity(),
            &reconcile_old,
            &reconcile_current,
            Some(&reconcile_pending),
        )
    });
    let refresh_store = store.clone();
    let refresh_barrier = Arc::clone(&barrier);
    let refresh_current = current.clone();
    let refresh = std::thread::spawn(move || {
        refresh_barrier.wait();
        refresh_store.record_success(
            identity(),
            ReleaseObservation::Latest {
                version: refresh_current.clone(),
                etag: Some("race".to_owned()),
            },
            refresh_current,
            Timestamp::from_millis(5),
        )
    });
    barrier.wait();
    assert!(matches!(
        reconcile.join().expect("reconcile thread"),
        Ok(ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied)
    ));
    refresh
        .join()
        .expect("refresh thread")
        .expect("release refresh");
    let state = store.load(identity()).expect("final race state");
    assert_eq!(state.observed_installed_version, Some(current));
    assert!(state.restart_needed);
    assert_eq!(state.external_restart, Some(pending));
}
