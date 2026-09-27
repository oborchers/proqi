//! Retirement belongs to an admitted fresh owner, not to automatic exec proof.

use std::path::{Path, PathBuf};

use crate::domain::{InstanceId, SessionId};

use super::{
    ExecutableIdentity, InputRecovery, PersistedPhase, RecoveryAdmission, RecoveryError,
    RecoveryFailure, RecoveryRecordPhase, RecoveryStage, SCHEMA_VERSION, remove_record,
    strict_load,
};

impl InputRecovery {
    pub(super) fn open_ordinary(
        path: PathBuf,
        session_id: SessionId,
        instance_id: InstanceId,
        pid: u32,
        executable: ExecutableIdentity,
    ) -> Self {
        Self::open_ordinary_with_removal(
            path,
            session_id,
            instance_id,
            pid,
            executable,
            remove_record,
        )
    }

    pub(super) fn open_ordinary_with_removal(
        path: PathBuf,
        session_id: SessionId,
        instance_id: InstanceId,
        pid: u32,
        executable: ExecutableIdentity,
        remove: impl FnOnce(&Path) -> Result<(), RecoveryError>,
    ) -> Self {
        let admission = match strict_load(&path) {
            Ok(None) => RecoveryAdmission::Fresh,
            Ok(Some(record))
                if record.schema_version == SCHEMA_VERSION && record.session_id == session_id =>
            {
                // The caller already owns this exact session after installation/schema
                // admission. Obsolete coordination is not evidence about the new process.
                // Never import its checkpoint or attempt budget as automatic exec proof.
                match remove(&path) {
                    Ok(()) => RecoveryAdmission::Retired {
                        executable_changed: record.executable != executable,
                        phase: match record.phase {
                            PersistedPhase::RecoveryPrepared { .. } => {
                                RecoveryRecordPhase::Prepared
                            }
                            PersistedPhase::Probation { .. } => RecoveryRecordPhase::Probation,
                            PersistedPhase::Healthy { .. } => RecoveryRecordPhase::Healthy,
                        },
                    },
                    Err(error) => RecoveryAdmission::Disabled(error.failure()),
                }
            }
            Ok(Some(_)) => RecoveryAdmission::Disabled(RecoveryFailure::MismatchedLineage),
            Err(error) => RecoveryAdmission::Disabled(error.failure()),
        };
        Self {
            path,
            instance_id,
            pid,
            executable: Some(executable),
            record: None,
            stage: RecoveryStage::Healthy,
            admission,
        }
    }

    pub(super) fn disabled(
        path: PathBuf,
        instance_id: InstanceId,
        pid: u32,
        reason: RecoveryFailure,
    ) -> Self {
        Self {
            path,
            instance_id,
            pid,
            executable: None,
            record: None,
            stage: RecoveryStage::Healthy,
            admission: RecoveryAdmission::Disabled(reason),
        }
    }
}
