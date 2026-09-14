//! Typed input-recovery identity, state, and failures.

use std::{fs::File, io::Read as _, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::domain::{InstanceId, RequestId, SessionId};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutableIdentity {
    pub(super) sha256: [u8; 32],
    pub(super) bytes: u64,
}

impl ExecutableIdentity {
    pub(crate) fn read(path: &Path) -> Result<Self, RecoveryError> {
        let mut file = File::open(path).map_err(|_| RecoveryError::ExecutableUnavailable)?;
        let metadata = file
            .metadata()
            .map_err(|_| RecoveryError::ExecutableUnavailable)?;
        if !metadata.is_file() {
            return Err(RecoveryError::ExecutableUnavailable);
        }
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| RecoveryError::ExecutableUnavailable)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        Ok(Self {
            sha256: digest.finalize().into(),
            bytes: metadata.len(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryExecProof {
    pub(crate) session_id: SessionId,
    pub(crate) lineage_id: RequestId,
    pub(crate) attempt_id: RequestId,
    pub(crate) previous_instance_id: InstanceId,
    pub(crate) pid: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryStage {
    Healthy,
    ConfirmedStall,
    RecoveryPrepared,
    Probation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryFailure {
    CircuitOpen,
    ExecFailed,
    ExecutableChanged,
    ExecutableUnavailable,
    MalformedProof,
    MismatchedLineage,
    PreparationFailed,
    ProbationFailed,
    RecordCorrupt,
    RecordUnavailable,
    StateUnsupported,
}

impl RecoveryFailure {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CircuitOpen => "circuit_open",
            Self::ExecFailed => "exec_failed",
            Self::ExecutableChanged => "executable_changed",
            Self::ExecutableUnavailable => "executable_unavailable",
            Self::MalformedProof => "malformed_proof",
            Self::MismatchedLineage => "mismatched_lineage",
            Self::PreparationFailed => "preparation_failed",
            Self::ProbationFailed => "probation_failed",
            Self::RecordCorrupt => "record_corrupt",
            Self::RecordUnavailable => "record_unavailable",
            Self::StateUnsupported => "state_unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryError {
    ExecutableUnavailable,
    MalformedProof,
    MismatchedLineage,
    RecordCorrupt,
    RecordUnavailable,
}

impl RecoveryError {
    pub(crate) const fn failure(self) -> RecoveryFailure {
        match self {
            Self::ExecutableUnavailable => RecoveryFailure::ExecutableUnavailable,
            Self::MalformedProof => RecoveryFailure::MalformedProof,
            Self::MismatchedLineage => RecoveryFailure::MismatchedLineage,
            Self::RecordCorrupt => RecoveryFailure::RecordCorrupt,
            Self::RecordUnavailable => RecoveryFailure::RecordUnavailable,
        }
    }
}
