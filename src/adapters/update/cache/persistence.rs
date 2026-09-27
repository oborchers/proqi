//! Atomic durable update-cache writes.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use crate::{domain::UpdateCacheState, ports::update::UpdateError};

use super::{
    MAX_STATE_BYTES, set_private_file_permissions, set_private_open_mode, state_error,
    sync_directory,
};

pub(super) fn write_atomic(
    directory: &Path,
    destination: &Path,
    state: &UpdateCacheState,
    fail_after_rename: bool,
) -> Result<(), UpdateError> {
    let bytes = serde_json::to_vec(state).map_err(|error| state_error(error.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_STATE_BYTES {
        return Err(UpdateError::State(
            "encoded update state exceeds its limit".to_owned(),
        ));
    }
    let (temporary, mut file) = reserve_temporary(directory)?;
    let result = (|| {
        file.write_all(&bytes).map_err(state_error)?;
        file.sync_all().map_err(state_error)?;
        fs::rename(&temporary, destination).map_err(state_error)?;
        if fail_after_rename {
            return Err(UpdateError::State(
                "injected failure after update state rename".to_owned(),
            ));
        }
        set_private_file_permissions(destination).map_err(state_error)?;
        sync_directory(directory)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn reserve_temporary(directory: &Path) -> Result<(PathBuf, File), UpdateError> {
    for suffix in 0..128_u16 {
        let path = directory.join(format!("state-{}-{suffix}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        set_private_open_mode(&mut options);
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(state_error(error)),
        }
    }
    Err(UpdateError::State(
        "could not reserve atomic update state".to_owned(),
    ))
}
