//! Private exact-session input recovery lineage and circuit-breaker state.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::domain::{InstanceId, RequestId, SessionId, Timestamp};
use crate::ports::runtime::InputRecoveryUiState;

mod admission;
mod model;
pub(crate) use model::{
    ExecutableIdentity, RecoveryAdmission, RecoveryError, RecoveryExecProof, RecoveryFailure,
    RecoveryRecordPhase, RecoveryStage,
};

const SCHEMA_VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 16 * 1024;
const RECOVERY_WINDOW_MS: i64 = 10 * 60 * 1_000;
const MAX_RECOVERIES_IN_WINDOW: usize = 2;
pub(crate) const PROBATION_PROGRESS_POLLS: u64 = 3;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryRecord {
    schema_version: u32,
    session_id: SessionId,
    lineage_id: RequestId,
    pid: u32,
    executable: ExecutableIdentity,
    attempts: Vec<Timestamp>,
    phase: PersistedPhase,
    ui_state: Option<InputRecoveryUiState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "stage")]
enum PersistedPhase {
    RecoveryPrepared {
        attempt_id: RequestId,
        previous_instance_id: InstanceId,
    },
    Probation {
        attempt_id: RequestId,
        previous_instance_id: InstanceId,
        current_instance_id: InstanceId,
    },
    Healthy {
        current_instance_id: InstanceId,
    },
}

pub(crate) struct InputRecovery {
    path: PathBuf,
    instance_id: InstanceId,
    pid: u32,
    executable: Option<ExecutableIdentity>,
    record: Option<RecoveryRecord>,
    stage: RecoveryStage,
    admission: RecoveryAdmission,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum StallDecision {
    Recover {
        attempt_count: usize,
    },
    FailClosed {
        reason: RecoveryFailure,
        attempt_count: usize,
    },
}

impl InputRecovery {
    // The terminal runner calls this only after canonical startup admission and
    // acquisition of the exact session lease. Records never confer ownership.
    pub(crate) fn open(
        runtime_root: &Path,
        session_id: SessionId,
        instance_id: InstanceId,
        executable: Result<ExecutableIdentity, RecoveryError>,
        startup: Result<Option<RecoveryExecProof>, RecoveryError>,
    ) -> Result<Self, RecoveryError> {
        let directory = runtime_root.join("input-recovery");
        let path = directory.join(format!("{session_id}.json"));
        let pid = std::process::id();
        if let Some(proof) = startup? {
            prepare_private_directory(&directory)?;
            Self::open_probation(path, session_id, instance_id, executable?, proof)
        } else {
            let executable = match executable {
                Ok(executable) => executable,
                Err(error) => {
                    return Ok(Self::disabled(path, instance_id, pid, error.failure()));
                }
            };
            if prepare_private_directory(&directory).is_err() {
                return Ok(Self::disabled(
                    path,
                    instance_id,
                    pid,
                    RecoveryFailure::RecordUnavailable,
                ));
            }
            Ok(Self::open_ordinary(
                path,
                session_id,
                instance_id,
                pid,
                executable,
            ))
        }
    }

    fn open_probation(
        path: PathBuf,
        session_id: SessionId,
        instance_id: InstanceId,
        executable: ExecutableIdentity,
        proof: RecoveryExecProof,
    ) -> Result<Self, RecoveryError> {
        let pid = std::process::id();
        let mut record = strict_load(&path)?.ok_or(RecoveryError::MismatchedLineage)?;
        let exact = record.schema_version == SCHEMA_VERSION
            && session_id == proof.session_id
            && record.session_id == proof.session_id
            && record.lineage_id == proof.lineage_id
            && record.pid == proof.pid
            && proof.pid == pid
            && record.executable == executable
            && matches!(
                record.phase,
                PersistedPhase::RecoveryPrepared { attempt_id, previous_instance_id }
                    if attempt_id == proof.attempt_id
                        && previous_instance_id == proof.previous_instance_id
            );
        if !exact {
            return Err(RecoveryError::MismatchedLineage);
        }
        record.phase = PersistedPhase::Probation {
            attempt_id: proof.attempt_id,
            previous_instance_id: proof.previous_instance_id,
            current_instance_id: instance_id,
        };
        write_atomic(&path, &record)?;
        Ok(Self {
            path,
            instance_id,
            pid,
            executable: Some(executable),
            record: Some(record),
            stage: RecoveryStage::Probation,
            admission: RecoveryAdmission::Probation,
        })
    }

    pub(crate) const fn admission(&self) -> RecoveryAdmission {
        self.admission
    }

    pub(crate) const fn stage(&self) -> RecoveryStage {
        self.stage
    }

    pub(crate) fn attempt_count(&self) -> usize {
        self.record
            .as_ref()
            .map_or(0, |record| record.attempts.len())
    }

    pub(crate) fn prove_healthy(&mut self, progress: u64) -> Result<bool, RecoveryError> {
        if self.stage != RecoveryStage::Probation || progress < PROBATION_PROGRESS_POLLS {
            return Ok(false);
        }
        let Some(record) = self.record.as_mut() else {
            return Err(RecoveryError::RecordCorrupt);
        };
        record.phase = PersistedPhase::Healthy {
            current_instance_id: self.instance_id,
        };
        record.ui_state = None;
        if let Err(error) = write_atomic(&self.path, record) {
            self.admission = RecoveryAdmission::Disabled(error.failure());
            self.stage = RecoveryStage::Healthy;
            return Err(error);
        }
        self.stage = RecoveryStage::Healthy;
        Ok(true)
    }

    pub(crate) fn confirm_stall(
        &mut self,
        now: Timestamp,
        attempt_id: RequestId,
        session_id: SessionId,
    ) -> StallDecision {
        if self.stage == RecoveryStage::Probation {
            return self.fail(RecoveryFailure::ProbationFailed);
        }
        if let RecoveryAdmission::Disabled(reason) = self.admission {
            return self.fail(reason);
        }
        if self.validate_current_record().is_err() {
            self.admission = RecoveryAdmission::Disabled(RecoveryFailure::RecordUnavailable);
            return self.fail(RecoveryFailure::RecordUnavailable);
        }
        let cutoff = now.as_millis().saturating_sub(recovery_window_ms());
        let mut attempts = self
            .record
            .as_ref()
            .map_or_else(Vec::new, |record| record.attempts.clone());
        attempts.retain(|attempt| attempt.as_millis() >= cutoff);
        if attempts.len() >= MAX_RECOVERIES_IN_WINDOW {
            return self.fail(RecoveryFailure::CircuitOpen);
        }
        attempts.push(now);
        let lineage_id = self
            .record
            .as_ref()
            .map_or(attempt_id, |record| record.lineage_id);
        let Some(executable) = self.executable.clone() else {
            self.admission = RecoveryAdmission::Disabled(RecoveryFailure::ExecutableUnavailable);
            return self.fail(RecoveryFailure::ExecutableUnavailable);
        };
        self.record = Some(RecoveryRecord {
            schema_version: SCHEMA_VERSION,
            session_id,
            lineage_id,
            pid: self.pid,
            executable,
            attempts,
            phase: PersistedPhase::RecoveryPrepared {
                attempt_id,
                previous_instance_id: self.instance_id,
            },
            ui_state: None,
        });
        self.stage = RecoveryStage::ConfirmedStall;
        StallDecision::Recover {
            attempt_count: self.attempt_count(),
        }
    }

    pub(crate) fn prepare(&mut self) -> Result<RecoveryExecProof, RecoveryError> {
        if self.stage != RecoveryStage::ConfirmedStall {
            return Err(RecoveryError::MismatchedLineage);
        }
        let record = self.record.as_ref().ok_or(RecoveryError::RecordCorrupt)?;
        if record.ui_state.is_none() {
            return Err(RecoveryError::RecordCorrupt);
        }
        let PersistedPhase::RecoveryPrepared {
            attempt_id,
            previous_instance_id,
        } = &record.phase
        else {
            return Err(RecoveryError::RecordCorrupt);
        };
        write_atomic(&self.path, record)?;
        self.stage = RecoveryStage::RecoveryPrepared;
        Ok(RecoveryExecProof {
            session_id: record.session_id,
            lineage_id: record.lineage_id,
            attempt_id: *attempt_id,
            previous_instance_id: *previous_instance_id,
            pid: record.pid,
        })
    }

    pub(crate) fn checkpoint(&mut self, state: InputRecoveryUiState) -> Result<(), RecoveryError> {
        if self.stage != RecoveryStage::ConfirmedStall {
            return Err(RecoveryError::MismatchedLineage);
        }
        let record = self.record.as_mut().ok_or(RecoveryError::RecordCorrupt)?;
        record.ui_state = Some(state);
        let size = serde_json::to_vec(record)
            .map_err(|_| RecoveryError::RecordCorrupt)?
            .len();
        if u64::try_from(size).unwrap_or(u64::MAX) > MAX_RECORD_BYTES {
            record.ui_state = None;
            return Err(RecoveryError::RecordCorrupt);
        }
        Ok(())
    }

    pub(crate) fn ui_state(&self) -> Option<&InputRecoveryUiState> {
        self.record.as_ref()?.ui_state.as_ref()
    }

    fn validate_current_record(&self) -> Result<(), RecoveryError> {
        match (&self.record, strict_load(&self.path)?) {
            (None, None) => Ok(()),
            (Some(expected), Some(actual)) if expected == &actual => Ok(()),
            _ => Err(RecoveryError::RecordCorrupt),
        }
    }

    fn fail(&self, reason: RecoveryFailure) -> StallDecision {
        StallDecision::FailClosed {
            reason,
            attempt_count: self.attempt_count(),
        }
    }
}

fn strict_load(path: &Path) -> Result<Option<RecoveryRecord>, RecoveryError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(RecoveryError::RecordUnavailable),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_RECORD_BYTES
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err(RecoveryError::RecordUnavailable);
    }
    let bytes = fs::read(path).map_err(|_| RecoveryError::RecordUnavailable)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| RecoveryError::RecordCorrupt)
}

fn write_atomic(path: &Path, record: &RecoveryRecord) -> Result<(), RecoveryError> {
    if strict_load(path).is_err() {
        return Err(RecoveryError::RecordUnavailable);
    }
    let bytes = serde_json::to_vec(record).map_err(|_| RecoveryError::RecordCorrupt)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RECORD_BYTES {
        return Err(RecoveryError::RecordCorrupt);
    }
    let directory = path.parent().ok_or(RecoveryError::RecordUnavailable)?;
    let (temporary, mut file) = reserve_temporary(directory)?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| RecoveryError::RecordUnavailable)?;
        fs::rename(&temporary, path).map_err(|_| RecoveryError::RecordUnavailable)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|_| RecoveryError::RecordUnavailable)?;
        File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| RecoveryError::RecordUnavailable)
    })();
    if result.is_err() {
        let _removed = fs::remove_file(&temporary);
    }
    result
}

fn reserve_temporary(directory: &Path) -> Result<(PathBuf, File), RecoveryError> {
    for suffix in 0..128_u16 {
        let path = directory.join(format!(
            "input-recovery-{}-{suffix}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(RecoveryError::RecordUnavailable),
        }
    }
    Err(RecoveryError::RecordUnavailable)
}

fn prepare_private_directory(path: &Path) -> Result<(), RecoveryError> {
    super::create_private_dir(path).map_err(|_| RecoveryError::RecordUnavailable)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| RecoveryError::RecordUnavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != rustix::process::getuid().as_raw()
    {
        return Err(RecoveryError::RecordUnavailable);
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| RecoveryError::RecordUnavailable)
}

fn remove_record(path: &Path) -> Result<(), RecoveryError> {
    strict_load(path)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(RecoveryError::RecordUnavailable),
    }
}

fn recovery_window_ms() -> i64 {
    if std::env::var_os("PROQI_TEST_INPUT_STALL").is_none() {
        return RECOVERY_WINDOW_MS;
    }
    std::env::var("PROQI_TEST_INPUT_RECOVERY_WINDOW_MS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0 && *value <= RECOVERY_WINDOW_MS)
        .unwrap_or(RECOVERY_WINDOW_MS)
}

#[cfg(test)]
#[path = "input_recovery/tests.rs"]
mod tests;
