use std::os::unix::fs::symlink;

use super::*;

fn assert_partial_acknowledgement(
    state: &FileUpdateStateStore,
    installation: InstallationIdentity,
    sessions: [crate::domain::SessionId; 2],
) {
    let durable = state.load(installation).expect("pending peer state");
    let remaining = durable.external_restart.expect("unfinished peer");
    assert!(remaining.manually_acknowledged(sessions[0]));
    assert_eq!(remaining.expectations()[0].session_id(), sessions[1]);
}

#[test]
fn exact_manual_resume_acknowledges_a_missing_replacement_after_session_lease() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([100; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("old observation");
    let mut ids = FakeIdGenerator::new(1_800_800_000_000);
    let (session_id, _proof) = record_pending_replacement(
        &state,
        installation,
        &observed,
        &current,
        &mut ids,
        std::process::id().saturating_add(1),
    );
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
        Some(session_id),
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(4),
        Duration::ZERO,
    )
    .expect("exact missing session enters manual recovery");
    admission
        .finish_after_store_ready()
        .expect("manual recovery retains pending state before session lease");
    let before_lease = state.load(installation).expect("pending before lease");
    assert!(before_lease.restart_needed);
    assert!(before_lease.external_restart.is_some());

    let session_lease = runtime
        .acquire_session(session_id)
        .expect("exact manual session lease");
    let directory = cache.join("updates").join(installation.to_string());
    let lock = directory.join("state.lock");
    std::fs::remove_file(&lock).expect("remove test-owned state lock");
    symlink(directory.join("state.json"), &lock).expect("inject unsafe state lock");
    let error = admission
        .finish_exact_resume(session_id)
        .expect_err("manual acknowledgement persistence must fail closed");
    assert!(format!("{error:?}").contains("update_state_failed"));
    let failed = state.load(installation).expect("unchanged failed state");
    assert!(failed.restart_needed);
    assert!(failed.external_restart.is_some());

    std::fs::remove_file(&lock).expect("remove injected state lock");
    admission
        .finish_exact_resume(session_id)
        .expect("retry leased exact session acknowledgement");
    let recovered = state.load(installation).expect("recovered state");
    assert!(!recovered.restart_needed);
    assert!(recovered.external_restart.is_none());
    drop((session_lease, admission));
}

#[test]
fn acknowledged_manual_session_can_reenter_while_an_exact_peer_remains_pending() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([102; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("old observation");
    let mut ids = FakeIdGenerator::new(1_800_850_000_000);
    let sessions = [ids.session_id(), ids.session_id()];
    let pending = ExternalRestartPending::new(
        current.clone(),
        ids.request_id(),
        sessions
            .into_iter()
            .map(|session_id| {
                ExternalRestartExpectation::new(
                    session_id,
                    ids.instance_id(),
                    std::process::id().saturating_add(1),
                    observed.clone(),
                )
            })
            .collect(),
    )
    .expect("pending cohort");
    state
        .reconcile_external_upgrade(installation, &observed, &current, Some(&pending))
        .expect("record pending cohort");
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );
    let mut first_admission = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        Some(sessions[0]),
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(5),
        Duration::ZERO,
    )
    .expect("first exact process enters manual recovery");
    let first_lease = runtime
        .acquire_session(sessions[0])
        .expect("first exact session lease");
    state.fail_next_write_after_rename();
    first_admission
        .finish_exact_resume(sessions[0])
        .expect_err("late persistence failure is reported");
    drop((first_lease, first_admission));

    assert_partial_acknowledgement(&state, installation, sessions);

    let admission = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        Some(sessions[0]),
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(6),
        Duration::ZERO,
    )
    .expect("fresh exact process reenters acknowledged session");
    let restored = runtime
        .acquire_session(sessions[0])
        .expect("fresh process owns exact session");
    assert_partial_acknowledgement(&state, installation, sessions);
    drop((restored, admission));
}

#[test]
fn repeated_manual_acknowledgement_is_idempotent_inside_one_admission() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([103; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("old observation");
    let mut ids = FakeIdGenerator::new(1_800_875_000_000);
    let (session_id, _proof) = record_pending_replacement(
        &state,
        installation,
        &observed,
        &current,
        &mut ids,
        std::process::id().saturating_add(1),
    );
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
        Some(session_id),
        &current,
        None,
        &mut ids,
        Timestamp::from_millis(7),
        Duration::ZERO,
    )
    .expect("exact process enters manual recovery");
    let lease = runtime
        .acquire_session(session_id)
        .expect("exact session lease");
    state.fail_next_write_after_rename();
    admission
        .finish_exact_resume(session_id)
        .expect_err("late persistence failure is reported");
    admission
        .finish_exact_resume(session_id)
        .expect("same admission recognizes exact committed state");
    let complete = state.load(installation).expect("completed state");
    assert!(!complete.restart_needed);
    assert!(complete.external_restart.is_none());
    drop((lease, admission));
}
