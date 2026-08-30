//! Authoritative installation-wide screenshot capture ownership.

use std::{fs, fs::File, path::PathBuf};

use fs4::TryLockError;

use crate::ports::runtime::{
    CaptureCoordinator, CaptureLease, CaptureLockError, CaptureOwnerInfo, InstanceInfo, Lease,
    RuntimeError,
};

use super::{
    FileRuntimeCoordinator, open_private_file, remove_if_exists, try_session_lock,
    write_private_json,
};

impl CaptureCoordinator for FileRuntimeCoordinator {
    type CaptureLease = FileCaptureLease;

    fn acquire_capture(
        &self,
        instance: &InstanceInfo,
    ) -> Result<Self::CaptureLease, CaptureLockError> {
        let (Some(control_protocol), Some(control_endpoint)) =
            (instance.control_protocol, instance.control_endpoint.clone())
        else {
            return Err(CaptureLockError::ControlUnavailable);
        };
        if instance.instance_id != self.instance_id
            || !bounded_text(&instance.version, 128)
            || !bounded_text(&control_endpoint, 1_024)
        {
            return Err(CaptureLockError::ControlUnavailable);
        }
        let file = open_private_file(&self.capture_lock_path()).map_err(capture_error)?;
        match try_session_lock(&file) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(CaptureLockError::Busy {
                    owner: read_owner(&self.capture_metadata_path()).map(Box::new),
                });
            }
            Err(TryLockError::Error(error)) => {
                return Err(CaptureLockError::Io(error.to_string()));
            }
        }
        let owner = CaptureOwnerInfo {
            instance_id: instance.instance_id,
            session_id: instance.session_id,
            pid: instance.pid,
            version: instance.version.clone(),
            capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
            control_protocol,
            control_endpoint,
            started_at: instance.started_at,
        };
        let metadata_path = self.capture_metadata_path();
        remove_if_exists(&metadata_path).map_err(capture_error)?;
        if let Err(error) = write_private_json(&metadata_path, &owner) {
            let _ = fs4::FileExt::unlock(&file);
            return Err(capture_error(error));
        }
        Ok(FileCaptureLease {
            file,
            metadata_path,
            owner,
        })
    }
}

/// Authoritative installation-wide screenshot lease.
#[derive(Debug)]
pub struct FileCaptureLease {
    file: File,
    metadata_path: PathBuf,
    owner: CaptureOwnerInfo,
}

impl Lease for FileCaptureLease {}

impl CaptureLease for FileCaptureLease {
    fn owner(&self) -> &CaptureOwnerInfo {
        &self.owner
    }
}

impl Drop for FileCaptureLease {
    fn drop(&mut self) {
        let _ = remove_if_exists(&self.metadata_path);
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

fn read_owner(path: &std::path::Path) -> Option<CaptureOwnerInfo> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 16 * 1024 {
        return None;
    }
    let owner: CaptureOwnerInfo = serde_json::from_slice(&bytes).ok()?;
    (bounded_text(&owner.version, 128)
        && bounded_text(&owner.control_endpoint, 1_024)
        && owner.pid > 0
        && owner.capture_protocol > 0
        && owner.control_protocol > 0)
        .then_some(owner)
}

fn bounded_text(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn capture_error(error: RuntimeError) -> CaptureLockError {
    match error {
        RuntimeError::Io(message) => CaptureLockError::Io(message),
        RuntimeError::MalformedMetadata(_) => CaptureLockError::MalformedMetadata,
        other => CaptureLockError::Io(other.to_string()),
    }
}
