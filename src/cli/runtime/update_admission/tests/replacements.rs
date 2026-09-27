use super::*;
use crate::{
    domain::{ExternalRestartExpectation, ExternalRestartPending},
    ports::runtime::UpdateReplacementContext,
};

mod manual;

fn record_pending_replacement(
    state: &FileUpdateStateStore,
    installation: InstallationIdentity,
    observed: &StableVersion,
    current: &StableVersion,
    ids: &mut FakeIdGenerator,
    previous_pid: u32,
) -> (crate::domain::SessionId, UpdateReplacementContext) {
    let operation_id = ids.request_id();
    let previous_instance_id = ids.instance_id();
    let session_id = ids.session_id();
    let pending = ExternalRestartPending::new(
        current.clone(),
        operation_id,
        vec![ExternalRestartExpectation::new(
            session_id,
            previous_instance_id,
            previous_pid,
            observed.clone(),
        )],
    )
    .expect("exact pending cohort");
    state
        .reconcile_external_upgrade(installation, observed, current, Some(&pending))
        .expect("pending external restart");
    (
        session_id,
        UpdateReplacementContext {
            operation_id,
            previous_instance_id,
            target_version: current.clone(),
        },
    )
}

fn assert_replacement_rejected(
    cache: &std::path::Path,
    runtime: &FileRuntimeCoordinator,
    installation: InstallationIdentity,
    resume: Option<crate::domain::SessionId>,
    current: &StableVersion,
    proof: Option<&UpdateReplacementContext>,
    ids: &mut FakeIdGenerator,
) {
    let result = admit_with_timeout(
        cache,
        runtime,
        installation,
        resume,
        current,
        proof,
        ids,
        Timestamp::from_millis(3),
        Duration::ZERO,
    );
    assert!(
        result
            .as_ref()
            .is_err_and(|error| format!("{error:?}").contains("update_convergence_active")),
        "mismatched replacement proof entered pending convergence"
    );
}

fn assert_ordinary_rejected(
    cache: &std::path::Path,
    runtime: &FileRuntimeCoordinator,
    installation: InstallationIdentity,
    current: &StableVersion,
    ids: &mut FakeIdGenerator,
) {
    assert_replacement_rejected(cache, runtime, installation, None, current, None, ids);
}

#[test]
fn exact_replacement_can_join_pending_restart_but_mismatches_cannot() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([98; 32]);
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("old observation");
    let existing_replacement = state
        .try_startup_lock(installation)
        .expect("startup lock")
        .expect("replacement admission");
    let mut ids = FakeIdGenerator::new(1_800_600_000_000);
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );
    let (session_id, replacement) = record_pending_replacement(
        &state,
        installation,
        &observed,
        &current,
        &mut ids,
        std::process::id(),
    );

    assert_ordinary_rejected(&cache, &runtime, installation, &current, &mut ids);

    let wrong_operation = UpdateReplacementContext {
        operation_id: ids.request_id(),
        ..replacement.clone()
    };
    let wrong_instance = UpdateReplacementContext {
        previous_instance_id: ids.instance_id(),
        ..replacement.clone()
    };
    let wrong_target = UpdateReplacementContext {
        target_version: StableVersion::parse("0.10.1").expect("wrong target"),
        ..replacement.clone()
    };
    for (resume, proof) in [
        (Some(session_id), &wrong_operation),
        (Some(session_id), &wrong_instance),
        (Some(session_id), &wrong_target),
        (Some(ids.session_id()), &replacement),
    ] {
        assert_replacement_rejected(
            &cache,
            &runtime,
            installation,
            resume,
            &current,
            Some(proof),
            &mut ids,
        );
    }

    let admission = admit_with_timeout(
        &cache,
        &runtime,
        installation,
        Some(session_id),
        &current,
        Some(&replacement),
        &mut ids,
        Timestamp::from_millis(3),
        Duration::ZERO,
    )
    .expect("exact replacement joins shared admission");
    drop((admission, existing_replacement));
}

#[test]
fn replacement_proof_requires_the_retained_operating_system_pid() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cache = temporary.path().join("cache");
    let installation = InstallationIdentity::from_digest([99; 32]);
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse("0.10.0").expect("current version");
    let state = FileUpdateStateStore::new(&cache).expect("update state");
    state
        .record_restart_state(installation, observed.clone(), true)
        .expect("old observation");
    let existing_replacement = state
        .try_startup_lock(installation)
        .expect("startup lock")
        .expect("replacement admission");
    let mut ids = FakeIdGenerator::new(1_800_700_000_000);
    let wrong_pid = std::process::id().saturating_add(1);
    let (session_id, proof) = record_pending_replacement(
        &state,
        installation,
        &observed,
        &current,
        &mut ids,
        wrong_pid,
    );
    let runtime = coordinator(
        temporary.path(),
        &mut ids,
        "0.10.0",
        installation,
        UPDATE_CONTROL_PROTOCOL_VERSION,
    );
    assert_replacement_rejected(
        &cache,
        &runtime,
        installation,
        Some(session_id),
        &current,
        Some(&proof),
        &mut ids,
    );
    drop(existing_replacement);
}

#[test]
fn later_external_version_cannot_erase_zero_live_or_partially_restored_pending_cohort() {
    for partial in [false, true] {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let cache = temporary.path().join("cache");
        let installation = InstallationIdentity::from_digest([101; 32]);
        let historical = StableVersion::parse("0.9.0").expect("historical version");
        let pending_target = StableVersion::parse("0.10.0").expect("pending target");
        let later = StableVersion::parse("0.11.0").expect("later version");
        let state = FileUpdateStateStore::new(&cache).expect("update state");
        state
            .record_restart_state(installation, historical.clone(), true)
            .expect("old observation");
        let mut ids = FakeIdGenerator::new(1_800_900_000_000);
        let operation_id = ids.request_id();
        let sessions = [ids.session_id(), ids.session_id()];
        let expectations = sessions
            .into_iter()
            .map(|session_id| {
                ExternalRestartExpectation::new(
                    session_id,
                    ids.instance_id(),
                    42,
                    historical.clone(),
                )
            })
            .collect();
        let pending =
            ExternalRestartPending::new(pending_target.clone(), operation_id, expectations)
                .expect("pending cohort");
        state
            .reconcile_external_upgrade(installation, &historical, &pending_target, Some(&pending))
            .expect("record pending cohort");
        let restored_runtime = coordinator(
            temporary.path(),
            &mut ids,
            "0.10.0",
            installation,
            UPDATE_CONTROL_PROTOCOL_VERSION,
        );
        let restored = partial.then(|| {
            restored_runtime
                .acquire_session(sessions[0])
                .expect("restored owner")
        });
        let later_runtime = coordinator(
            temporary.path(),
            &mut ids,
            "0.11.0",
            installation,
            UPDATE_CONTROL_PROTOCOL_VERSION,
        );

        let Err(error) = admit_with_timeout(
            &cache,
            &later_runtime,
            installation,
            None,
            &later,
            None,
            &mut ids,
            Timestamp::from_millis(5),
            Duration::ZERO,
        ) else {
            panic!("later external version must preserve unfinished cohort");
        };
        let diagnostic = format!("{error:?}");
        assert!(diagnostic.contains("external_upgrade_pending"));
        for session in sessions {
            assert!(diagnostic.contains(&session.to_string()));
        }
        let unchanged = state.load(installation).expect("unchanged pending state");
        assert_eq!(unchanged.observed_installed_version, Some(pending_target));
        assert!(unchanged.restart_needed);
        assert_eq!(unchanged.external_restart, Some(pending));
        drop(restored);
    }
}
