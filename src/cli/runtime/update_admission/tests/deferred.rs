use std::os::unix::fs::symlink;

use super::*;

#[test]
fn cache_persistence_failure_keeps_old_state_and_retries_idempotently() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([100; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("stale observation");
    let mut ids = FakeIdGenerator::new(1_800_800_000_000);
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );
    let mut admission = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(1),
        Duration::ZERO,
    )
    .expect("deferred external adoption");
    let directory = cache.join("updates").join(installation.to_string());
    let lock = directory.join("state.lock");
    fs::remove_file(&lock).expect("remove test-owned state lock");
    symlink(directory.join("state.json"), &lock).expect("inject unsafe state lock");

    let error = admission
        .finish_after_store_ready()
        .expect_err("cache persistence must fail closed");
    assert!(format!("{error:?}").contains("update_state_failed"));
    let unchanged = state.load(installation).expect("unchanged state");
    assert_eq!(unchanged.observed_installed_version, Some(observed.clone()));
    assert!(unchanged.restart_needed);
    drop(admission);

    fs::remove_file(&lock).expect("remove injected state lock");
    let mut retry = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        None,
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(2),
        Duration::ZERO,
    )
    .expect("retry deferred adoption");
    retry
        .finish_after_store_ready()
        .expect("retry cache persistence");
    drop(retry);
    let reconciled = state.load(installation).expect("reconciled state");
    assert_eq!(reconciled.observed_installed_version, Some(current));
    assert!(!reconciled.restart_needed);
}
