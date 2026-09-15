use std::{fs, os::unix::fs::PermissionsExt as _};

use crate::{
    adapters::runtime::SystemIdGenerator,
    domain::Timestamp,
    ports::environment::IdGenerator as _,
    ports::runtime::{
        InputRecoveryBoardViewport, InputRecoveryMode, InputRecoveryScrollAnchor,
        InputRecoverySelection, InputRecoveryUiState,
    },
};

use super::{
    ExecutableIdentity, InputRecovery, RecoveryError, RecoveryExecProof, RecoveryFailure,
    RecoveryStage, StallDecision,
};

fn identity(byte: u8) -> ExecutableIdentity {
    ExecutableIdentity {
        sha256: [byte; 32],
        bytes: 100,
    }
}

fn ui_state() -> InputRecoveryUiState {
    InputRecoveryUiState {
        mode: InputRecoveryMode::Board,
        focused_thought: None,
        insertion_index: 0,
        insertion_focused: false,
        compose_editor_visible: false,
        editor: None,
        selection: InputRecoverySelection::default(),
        expanded_folds: Vec::new(),
        board_viewport: InputRecoveryBoardViewport {
            follows_focus: true,
            anchor: InputRecoveryScrollAnchor::Start,
        },
    }
}

#[test]
fn prepared_replacement_enters_probation_and_requires_real_progress() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let first_instance = ids.instance_id();
    let attempt_id = ids.request_id();
    let executable = identity(7);
    let mut recovery = InputRecovery::open(
        temporary.path(),
        session_id,
        first_instance,
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("ordinary startup");

    assert_eq!(
        recovery.confirm_stall(Timestamp::from_millis(1), attempt_id, session_id),
        StallDecision::Recover { attempt_count: 1 }
    );
    recovery.checkpoint(ui_state()).expect("UI checkpoint");
    let proof = recovery.prepare().expect("prepared record");
    let second_instance = ids.instance_id();
    let mut probation = InputRecovery::open(
        temporary.path(),
        session_id,
        second_instance,
        Ok(executable),
        Ok(Some(proof)),
    )
    .expect("replacement startup");

    assert_eq!(probation.stage(), RecoveryStage::Probation);
    assert!(!probation.prove_healthy(2).expect("below threshold"));
    assert!(probation.prove_healthy(3).expect("threshold reached"));
    assert_eq!(probation.stage(), RecoveryStage::Healthy);
}

#[test]
fn probation_stall_fails_closed_without_another_attempt() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let first_instance = ids.instance_id();
    let attempt_id = ids.request_id();
    let executable = identity(8);
    let mut first = InputRecovery::open(
        temporary.path(),
        session_id,
        first_instance,
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("ordinary startup");
    assert!(matches!(
        first.confirm_stall(Timestamp::from_millis(1), attempt_id, session_id),
        StallDecision::Recover { .. }
    ));
    first.checkpoint(ui_state()).expect("UI checkpoint");
    let proof = first.prepare().expect("prepared record");
    let mut probation = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Ok(executable),
        Ok(Some(proof)),
    )
    .expect("replacement startup");

    assert_eq!(
        probation.confirm_stall(Timestamp::from_millis(2), ids.request_id(), session_id),
        StallDecision::FailClosed {
            reason: RecoveryFailure::ProbationFailed,
            attempt_count: 1,
        }
    );
}

#[test]
fn rolling_window_allows_two_attempts_and_releases_old_incidents() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let executable = identity(9);
    let mut recovery = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("ordinary startup");

    let first = ids.request_id();
    assert!(matches!(
        recovery.confirm_stall(Timestamp::from_millis(0), first, session_id),
        StallDecision::Recover { attempt_count: 1 }
    ));
    recovery.checkpoint(ui_state()).expect("UI checkpoint");
    let first_proof = recovery.prepare().expect("first prepared record");
    let mut recovery = replacement_then_healthy(
        temporary.path(),
        session_id,
        &mut ids,
        executable.clone(),
        first_proof,
    );
    let second = ids.request_id();
    assert!(matches!(
        recovery.confirm_stall(Timestamp::from_millis(10), second, session_id),
        StallDecision::Recover { attempt_count: 2 }
    ));
    recovery.checkpoint(ui_state()).expect("UI checkpoint");
    let second_proof = recovery.prepare().expect("second prepared record");
    let mut recovery = replacement_then_healthy(
        temporary.path(),
        session_id,
        &mut ids,
        executable,
        second_proof,
    );
    assert_eq!(
        recovery.confirm_stall(Timestamp::from_millis(20), ids.request_id(), session_id),
        StallDecision::FailClosed {
            reason: RecoveryFailure::CircuitOpen,
            attempt_count: 2,
        }
    );
    assert!(matches!(
        recovery.confirm_stall(
            Timestamp::from_millis(10 * 60 * 1_000 + 1),
            ids.request_id(),
            session_id,
        ),
        StallDecision::Recover { attempt_count: 2 }
    ));
}

#[test]
fn exact_session_lineage_pid_and_executable_are_required() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let mut recovery = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Ok(identity(1)),
        Ok(None),
    )
    .expect("ordinary startup");
    assert!(matches!(
        recovery.confirm_stall(Timestamp::from_millis(1), ids.request_id(), session_id),
        StallDecision::Recover { .. }
    ));
    recovery.checkpoint(ui_state()).expect("UI checkpoint");
    let proof = recovery.prepare().expect("prepared record");
    let wrong = RecoveryExecProof {
        session_id: ids.session_id(),
        ..proof
    };

    assert_eq!(
        InputRecovery::open(
            temporary.path(),
            session_id,
            ids.instance_id(),
            Ok(identity(1)),
            Ok(Some(wrong)),
        )
        .err(),
        Some(RecoveryError::MismatchedLineage)
    );
    assert_eq!(
        InputRecovery::open(
            temporary.path(),
            session_id,
            ids.instance_id(),
            Ok(identity(2)),
            Ok(Some(proof)),
        )
        .err(),
        Some(RecoveryError::MismatchedLineage)
    );
}

#[test]
fn malformed_or_public_record_disables_ordinary_automatic_recovery() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let directory = temporary.path().join("input-recovery");
    fs::create_dir(&directory).expect("recovery directory");
    let path = directory.join(format!("{session_id}.json"));
    fs::write(&path, b"not json").expect("malformed record");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("public mode");
    let mut recovery = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Ok(identity(1)),
        Ok(None),
    )
    .expect("ordinary startup remains available");

    assert_eq!(
        recovery.confirm_stall(Timestamp::from_millis(1), ids.request_id(), session_id),
        StallDecision::FailClosed {
            reason: RecoveryFailure::RecordUnavailable,
            attempt_count: 0,
        }
    );
}

#[test]
fn unavailable_executable_disables_recovery_without_blocking_healthy_startup() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let mut recovery = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Err(RecoveryError::ExecutableUnavailable),
        Ok(None),
    )
    .expect("ordinary startup remains available");

    assert_eq!(recovery.stage(), RecoveryStage::Healthy);
    assert_eq!(
        recovery.confirm_stall(Timestamp::from_millis(1), ids.request_id(), session_id),
        StallDecision::FailClosed {
            reason: RecoveryFailure::ExecutableUnavailable,
            attempt_count: 0,
        }
    );
}

#[test]
fn replacement_proof_must_name_the_live_session_even_at_a_matching_record_path() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let recorded_session = ids.session_id();
    let live_session = ids.session_id();
    let executable = identity(3);
    let mut recovery = InputRecovery::open(
        temporary.path(),
        recorded_session,
        ids.instance_id(),
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("ordinary startup");
    assert!(matches!(
        recovery.confirm_stall(
            Timestamp::from_millis(1),
            ids.request_id(),
            recorded_session
        ),
        StallDecision::Recover { .. }
    ));
    recovery.checkpoint(ui_state()).expect("UI checkpoint");
    let proof = recovery.prepare().expect("prepared record");
    let directory = temporary.path().join("input-recovery");
    fs::rename(
        directory.join(format!("{recorded_session}.json")),
        directory.join(format!("{live_session}.json")),
    )
    .expect("move record to live session path");

    assert_eq!(
        InputRecovery::open(
            temporary.path(),
            live_session,
            ids.instance_id(),
            Ok(executable),
            Ok(Some(proof)),
        )
        .err(),
        Some(RecoveryError::MismatchedLineage)
    );
}

#[test]
fn ordinary_startup_preserves_a_record_for_another_exact_session() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let recorded_session = ids.session_id();
    let live_session = ids.session_id();
    let executable = identity(6);
    let mut recovery = InputRecovery::open(
        temporary.path(),
        recorded_session,
        ids.instance_id(),
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("ordinary startup");
    assert!(matches!(
        recovery.confirm_stall(
            Timestamp::from_millis(1),
            ids.request_id(),
            recorded_session
        ),
        StallDecision::Recover { .. }
    ));
    recovery.checkpoint(ui_state()).expect("UI checkpoint");
    let _proof = recovery.prepare().expect("prepared record");
    let directory = temporary.path().join("input-recovery");
    let live_path = directory.join(format!("{live_session}.json"));
    fs::rename(
        directory.join(format!("{recorded_session}.json")),
        &live_path,
    )
    .expect("move record to live session path");
    let mut ordinary = InputRecovery::open(
        temporary.path(),
        live_session,
        ids.instance_id(),
        Ok(executable),
        Ok(None),
    )
    .expect("ordinary startup remains available");

    assert_eq!(
        ordinary.confirm_stall(Timestamp::from_millis(2), ids.request_id(), live_session),
        StallDecision::FailClosed {
            reason: RecoveryFailure::MismatchedLineage,
            attempt_count: 0,
        }
    );
    assert!(
        live_path.exists(),
        "mismatched exact-session record retained"
    );
}

#[test]
fn recovery_budgets_are_isolated_by_exact_session() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let first_session = ids.session_id();
    let second_session = ids.session_id();
    let executable = identity(4);
    let mut first = InputRecovery::open(
        temporary.path(),
        first_session,
        ids.instance_id(),
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("first session");
    for time in [1, 2] {
        assert!(matches!(
            first.confirm_stall(
                Timestamp::from_millis(time),
                ids.request_id(),
                first_session
            ),
            StallDecision::Recover { .. }
        ));
        first.checkpoint(ui_state()).expect("UI checkpoint");
        let proof = first.prepare().expect("prepared record");
        first = replacement_then_healthy(
            temporary.path(),
            first_session,
            &mut ids,
            executable.clone(),
            proof,
        );
    }
    assert!(matches!(
        first.confirm_stall(Timestamp::from_millis(3), ids.request_id(), first_session),
        StallDecision::FailClosed {
            reason: RecoveryFailure::CircuitOpen,
            ..
        }
    ));

    let mut second = InputRecovery::open(
        temporary.path(),
        second_session,
        ids.instance_id(),
        Ok(executable),
        Ok(None),
    )
    .expect("second session");
    assert_eq!(
        second.confirm_stall(Timestamp::from_millis(3), ids.request_id(), second_session),
        StallDecision::Recover { attempt_count: 1 }
    );
}

#[test]
fn ordinary_launch_discards_valid_stale_lineage_and_ignores_orphan_temporary_file() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session_id = ids.session_id();
    let executable = identity(5);
    let mut interrupted = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Ok(executable.clone()),
        Ok(None),
    )
    .expect("ordinary startup");
    assert!(matches!(
        interrupted.confirm_stall(Timestamp::from_millis(1), ids.request_id(), session_id),
        StallDecision::Recover { .. }
    ));
    interrupted.checkpoint(ui_state()).expect("UI checkpoint");
    let _proof = interrupted.prepare().expect("prepared record");
    fs::write(
        temporary.path().join("input-recovery/orphan.tmp"),
        b"interrupted write",
    )
    .expect("orphan temporary file");

    let mut ordinary = InputRecovery::open(
        temporary.path(),
        session_id,
        ids.instance_id(),
        Ok(executable),
        Ok(None),
    )
    .expect("ordinary relaunch");
    assert_eq!(
        ordinary.confirm_stall(Timestamp::from_millis(2), ids.request_id(), session_id),
        StallDecision::Recover { attempt_count: 1 }
    );
}

fn replacement_then_healthy(
    runtime: &std::path::Path,
    session_id: crate::domain::SessionId,
    ids: &mut SystemIdGenerator,
    executable: ExecutableIdentity,
    proof: RecoveryExecProof,
) -> InputRecovery {
    let mut recovery = InputRecovery::open(
        runtime,
        session_id,
        ids.instance_id(),
        Ok(executable),
        Ok(Some(proof)),
    )
    .expect("replacement startup");
    assert!(recovery.prove_healthy(3).expect("probation progress"));
    recovery
}
