//! Fresh admitted startup retires obsolete recovery coordination, never exec proof.

use crate::{
    adapters::runtime::SystemIdGenerator, domain::Timestamp, ports::environment::IdGenerator as _,
};

use super::{InputRecovery, RecoveryStage, StallDecision, identity, ui_state};

#[test]
fn ordinary_startup_after_executable_upgrade_retires_healthy_prior_lineage() {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session = ids.session_id();
    let mut old = InputRecovery::open(
        temporary.path(),
        session,
        ids.instance_id(),
        Ok(identity(1)),
        Ok(None),
    )
    .expect("old ordinary startup");
    assert_eq!(
        old.confirm_stall(Timestamp::from_millis(1), ids.request_id(), session),
        StallDecision::Recover { attempt_count: 1 },
    );
    old.checkpoint(ui_state()).expect("old checkpoint");
    let proof = old.prepare().expect("old preparation");
    let old =
        super::replacement_then_healthy(temporary.path(), session, &mut ids, identity(1), proof);
    drop(old);

    let mut upgraded = InputRecovery::open(
        temporary.path(),
        session,
        ids.instance_id(),
        Ok(identity(2)),
        Ok(None),
    )
    .expect("admitted upgraded ordinary startup");
    assert_eq!(upgraded.stage(), RecoveryStage::Healthy);
    assert_eq!(
        upgraded.confirm_stall(Timestamp::from_millis(2), ids.request_id(), session),
        StallDecision::Recover { attempt_count: 1 },
        "an obsolete exact-session record must not disable the new owner",
    );
    assert!(
        !temporary
            .path()
            .join("input-recovery")
            .join(format!("{session}.json"))
            .exists()
    );
}

#[test]
fn fresh_owner_retires_each_supported_phase_without_importing_checkpoint_or_budget() {
    use super::super::{RecoveryAdmission, RecoveryRecordPhase};
    for phase in [
        RecoveryRecordPhase::Prepared,
        RecoveryRecordPhase::Probation,
        RecoveryRecordPhase::Healthy,
    ] {
        for executable_changed in [false, true] {
            let (temporary, session, proof) = prepared();
            let mut ids = SystemIdGenerator;
            advance_phase(temporary.path(), session, proof, phase);
            let mut owner = InputRecovery::open(
                temporary.path(),
                session,
                ids.instance_id(),
                Ok(identity(if executable_changed { 2 } else { 1 })),
                Ok(None),
            )
            .expect("fresh admitted owner");
            assert_eq!(
                owner.admission,
                RecoveryAdmission::Retired {
                    executable_changed,
                    phase
                }
            );
            assert!(owner.ui_state().is_none());
            assert_eq!(owner.attempt_count(), 0);
            assert!(
                !owner
                    .prove_healthy(3)
                    .expect("ordinary startup is not recovery proof")
            );
            assert_eq!(
                owner.confirm_stall(Timestamp::from_millis(2), ids.request_id(), session),
                StallDecision::Recover { attempt_count: 1 }
            );
        }
    }
}

fn advance_phase(
    root: &std::path::Path,
    session: crate::domain::SessionId,
    proof: super::super::RecoveryExecProof,
    phase: super::super::RecoveryRecordPhase,
) {
    use super::super::RecoveryRecordPhase;
    if phase == RecoveryRecordPhase::Prepared {
        return;
    }
    let mut ids = SystemIdGenerator;
    let mut replacement = InputRecovery::open(
        root,
        session,
        ids.instance_id(),
        Ok(identity(1)),
        Ok(Some(proof)),
    )
    .expect("actual probation entry");
    if phase == RecoveryRecordPhase::Healthy {
        assert!(replacement.prove_healthy(3).expect("actual health proof"));
    }
}

#[test]
fn automatic_exec_rejects_each_wrong_identity_without_retiring_the_record() {
    use super::super::{RecoveryError, RecoveryExecProof};
    let (temporary, session, proof) = prepared();
    let path = temporary
        .path()
        .join("input-recovery")
        .join(format!("{session}.json"));
    let before = std::fs::read(&path).expect("prepared bytes");
    let mut ids = SystemIdGenerator;
    for wrong in [
        RecoveryExecProof {
            session_id: ids.session_id(),
            ..proof
        },
        RecoveryExecProof {
            lineage_id: ids.request_id(),
            ..proof
        },
        RecoveryExecProof {
            attempt_id: ids.request_id(),
            ..proof
        },
        RecoveryExecProof {
            previous_instance_id: ids.instance_id(),
            ..proof
        },
        RecoveryExecProof {
            pid: proof.pid.saturating_add(1),
            ..proof
        },
    ] {
        assert_eq!(
            InputRecovery::open(
                temporary.path(),
                session,
                ids.instance_id(),
                Ok(identity(1)),
                Ok(Some(wrong))
            )
            .err(),
            Some(RecoveryError::MismatchedLineage)
        );
        assert_eq!(std::fs::read(&path).expect("retained bytes"), before);
    }
    assert_eq!(
        InputRecovery::open(
            temporary.path(),
            session,
            ids.instance_id(),
            Ok(identity(2)),
            Ok(Some(proof))
        )
        .err(),
        Some(RecoveryError::MismatchedLineage)
    );
    let other_root = tempfile::tempdir().expect("different state root");
    assert_eq!(
        InputRecovery::open(
            other_root.path(),
            session,
            ids.instance_id(),
            Ok(identity(1)),
            Ok(Some(proof))
        )
        .err(),
        Some(RecoveryError::MismatchedLineage)
    );
    assert_eq!(std::fs::read(&path).expect("retained bytes"), before);
}

#[test]
fn unsupported_schema_and_foreign_session_are_never_retired() {
    use super::super::RecoveryFailure;
    for foreign_session in [false, true] {
        let (temporary, session, _proof) = prepared();
        let mut ids = SystemIdGenerator;
        let path = temporary
            .path()
            .join("input-recovery")
            .join(format!("{session}.json"));
        let mut record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("record")).expect("JSON");
        if foreign_session {
            record["session_id"] = serde_json::json!(ids.session_id());
        } else {
            record["schema_version"] = serde_json::json!(2);
        }
        let before = serde_json::to_vec(&record).expect("record bytes");
        std::fs::write(&path, &before).expect("invalid identity fixture");
        let mut owner = InputRecovery::open(
            temporary.path(),
            session,
            ids.instance_id(),
            Ok(identity(2)),
            Ok(None),
        )
        .expect("healthy launch");
        assert_eq!(
            owner.confirm_stall(Timestamp::from_millis(2), ids.request_id(), session),
            StallDecision::FailClosed {
                reason: RecoveryFailure::MismatchedLineage,
                attempt_count: 0
            }
        );
        assert_eq!(std::fs::read(&path).expect("retained bytes"), before);
    }
}

#[test]
fn malformed_incomplete_public_oversized_and_symlink_records_remain_untrusted() {
    use super::super::RecoveryAdmission;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    for case in ["malformed", "incomplete", "public", "oversized", "symlink"] {
        let (temporary, session, _proof) = prepared();
        let mut ids = SystemIdGenerator;
        let path = temporary
            .path()
            .join("input-recovery")
            .join(format!("{session}.json"));
        let target = temporary.path().join("unrelated");
        std::fs::write(&target, b"untouched unrelated bytes").expect("unrelated target");
        match case {
            "malformed" => std::fs::write(&path, b"{").expect("malformed fixture"),
            "incomplete" => std::fs::write(&path, b"{}").expect("incomplete fixture"),
            "public" => std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .expect("public fixture"),
            "oversized" => {
                std::fs::write(&path, vec![b' '; 16 * 1024 + 1]).expect("oversized fixture");
            }
            "symlink" => {
                std::fs::remove_file(&path).expect("replace own record");
                symlink(&target, &path).expect("symlink fixture");
            }
            _ => panic!("unknown test case"),
        }
        let before = std::fs::read(&path).expect("fixture bytes");
        let owner = InputRecovery::open(
            temporary.path(),
            session,
            ids.instance_id(),
            Ok(identity(2)),
            Ok(None),
        )
        .expect("healthy launch");
        assert!(
            matches!(owner.admission, RecoveryAdmission::Disabled(_)),
            "{case}"
        );
        assert_eq!(std::fs::read(&path).expect("retained bytes"), before);
        if case == "symlink" {
            assert!(
                std::fs::symlink_metadata(&path)
                    .expect("retained symlink")
                    .is_symlink()
            );
        }
        assert_eq!(
            std::fs::read(&target).expect("unrelated bytes"),
            b"untouched unrelated bytes"
        );
    }
}

#[test]
fn failed_record_removal_disables_recovery_and_retains_exact_record() {
    use super::super::{RecoveryAdmission, RecoveryError, RecoveryFailure};
    let (temporary, session, _proof) = prepared();
    let mut ids = SystemIdGenerator;
    let directory = temporary.path().join("input-recovery");
    let path = directory.join(format!("{session}.json"));
    let before = std::fs::read(&path).expect("record bytes");
    let owner = InputRecovery::open_ordinary_with_removal(
        path.clone(),
        session,
        ids.instance_id(),
        std::process::id(),
        identity(2),
        |requested| {
            assert_eq!(requested, path);
            Err(RecoveryError::RecordUnavailable)
        },
    );
    assert_eq!(
        owner.admission,
        RecoveryAdmission::Disabled(RecoveryFailure::RecordUnavailable)
    );
    assert_eq!(std::fs::read(path).expect("record retained"), before);
}

fn prepared() -> (
    tempfile::TempDir,
    crate::domain::SessionId,
    super::super::RecoveryExecProof,
) {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let mut ids = SystemIdGenerator;
    let session = ids.session_id();
    let mut owner = InputRecovery::open(
        temporary.path(),
        session,
        ids.instance_id(),
        Ok(identity(1)),
        Ok(None),
    )
    .expect("ordinary owner");
    assert_eq!(
        owner.confirm_stall(Timestamp::from_millis(1), ids.request_id(), session),
        StallDecision::Recover { attempt_count: 1 }
    );
    owner.checkpoint(ui_state()).expect("checkpoint");
    let proof = owner.prepare().expect("prepared record");
    (temporary, session, proof)
}
