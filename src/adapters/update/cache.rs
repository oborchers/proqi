//! Private atomic installation-wide update state and operation locks.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use fs4::{FileExt, TryLockError};

use crate::{
    domain::{InstallationIdentity, StableVersion, Timestamp, UpdateCacheState},
    ports::update::{
        ReleaseObservation, UpdateError, UpdateLease, UpdateLockKind, UpdateStateStore,
    },
};

const MAX_STATE_BYTES: u64 = 16 * 1024;
const MAX_ETAG_BYTES: usize = 256;
const STATE_LOCK_ATTEMPTS: usize = 100;

/// Filesystem-backed update state rooted in the platform cache directory.
#[derive(Clone, Debug)]
pub struct FileUpdateStateStore {
    root: PathBuf,
}

impl FileUpdateStateStore {
    /// Prepare a private update cache root.
    ///
    /// # Errors
    ///
    /// Rejects relative or symlinked roots and permission failures.
    pub fn new(cache_dir: &Path) -> Result<Self, UpdateError> {
        if !cache_dir.is_absolute() {
            return Err(UpdateError::State(
                "cache directory must be absolute".to_owned(),
            ));
        }
        prepare_private_dir(cache_dir)?;
        let root = cache_dir.join("updates");
        prepare_private_dir(&root)?;
        Ok(Self { root })
    }

    fn installation_dir(&self, installation: InstallationIdentity) -> Result<PathBuf, UpdateError> {
        let path = self.root.join(installation.to_string());
        prepare_private_dir(&path)?;
        Ok(path)
    }

    fn state_path(&self, installation: InstallationIdentity) -> Result<PathBuf, UpdateError> {
        Ok(self.installation_dir(installation)?.join("state.json"))
    }

    fn mutate(
        &self,
        installation: InstallationIdentity,
        change: impl FnOnce(&mut UpdateCacheState) -> Result<(), UpdateError>,
    ) -> Result<UpdateCacheState, UpdateError> {
        let directory = self.installation_dir(installation)?;
        let lock = lock_state(&directory.join("state.lock"))?;
        let state_path = directory.join("state.json");
        let mut state = load_path(&state_path)?;
        change(&mut state)?;
        write_atomic(&directory, &state_path, &state)?;
        drop(lock);
        Ok(state)
    }
}

impl UpdateStateStore for FileUpdateStateStore {
    fn load(&self, installation: InstallationIdentity) -> Result<UpdateCacheState, UpdateError> {
        load_path(&self.state_path(installation)?)
    }

    fn try_lock(
        &self,
        installation: InstallationIdentity,
        kind: UpdateLockKind,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        let name = match kind {
            UpdateLockKind::Refresh => "refresh.lock",
            UpdateLockKind::Prompt => "prompt.lock",
            UpdateLockKind::Installer => "installer.lock",
        };
        let file = open_private_file(&self.installation_dir(installation)?.join(name))?;
        match FileExt::try_lock(&file) {
            Ok(()) => Ok(Some(Box::new(FileUpdateLease { file }))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(state_error(error)),
        }
    }

    fn record_success(
        &self,
        installation: InstallationIdentity,
        observed: ReleaseObservation,
        installed: StableVersion,
        checked_at: Timestamp,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.mutate(installation, |state| {
            match observed {
                ReleaseObservation::Latest { version, etag } => {
                    merge_latest(state, version, etag);
                }
                ReleaseObservation::NotModified if state.latest_stable.is_none() => {
                    return Err(UpdateError::InvalidResponse);
                }
                ReleaseObservation::NotModified => {}
            }
            state.last_checked_at = Some(checked_at);
            state.dismissed_version = None;
            state.observed_installed_version = Some(installed);
            Ok(())
        })
    }

    fn dismiss(
        &self,
        installation: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.mutate(installation, |state| {
            state.dismissed_version = Some(version);
            Ok(())
        })
    }

    fn skip(
        &self,
        installation: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.mutate(installation, |state| {
            state.skipped_version = Some(version);
            Ok(())
        })
    }

    fn record_restart_state(
        &self,
        installation: InstallationIdentity,
        installed: StableVersion,
        restart_needed: bool,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.mutate(installation, |state| {
            state.observed_installed_version = Some(installed);
            state.restart_needed = restart_needed;
            Ok(())
        })
    }
}

fn merge_latest(state: &mut UpdateCacheState, version: StableVersion, etag: Option<String>) {
    if state.skipped_version.as_ref() != Some(&version) {
        state.skipped_version = None;
    }
    state.latest_stable = Some(version);
    state.etag = etag.filter(|value| valid_etag(value));
}

struct FileUpdateLease {
    file: File,
}

impl UpdateLease for FileUpdateLease {}

impl Drop for FileUpdateLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn load_path(path: &Path) -> Result<UpdateCacheState, UpdateError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UpdateCacheState::default());
        }
        Err(error) => return Err(state_error(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UpdateError::State(
            "update state is not a regular file".to_owned(),
        ));
    }
    if metadata.len() > MAX_STATE_BYTES {
        return Ok(UpdateCacheState::default());
    }
    set_private_file_permissions(path).map_err(state_error)?;
    let bytes = fs::read(path).map_err(state_error)?;
    let state: UpdateCacheState = match serde_json::from_slice(&bytes) {
        Ok(state) => state,
        Err(_) => return Ok(UpdateCacheState::default()),
    };
    if state.etag.as_deref().is_some_and(|etag| !valid_etag(etag)) {
        return Ok(UpdateCacheState::default());
    }
    Ok(state)
}

fn lock_state(path: &Path) -> Result<FileUpdateLease, UpdateError> {
    let file = open_private_file(path)?;
    for _ in 0..STATE_LOCK_ATTEMPTS {
        match FileExt::try_lock(&file) {
            Ok(()) => return Ok(FileUpdateLease { file }),
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(2)),
            Err(TryLockError::Error(error)) => return Err(state_error(error)),
        }
    }
    Err(UpdateError::State(
        "update state lock remained busy".to_owned(),
    ))
}

fn write_atomic(
    directory: &Path,
    destination: &Path,
    state: &UpdateCacheState,
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

fn open_private_file(path: &Path) -> Result<File, UpdateError> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(UpdateError::State(
            "update lock cannot be a symlink".to_owned(),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    set_private_open_mode(&mut options);
    let file = options.open(path).map_err(state_error)?;
    set_private_file_permissions(path).map_err(state_error)?;
    Ok(file)
}

fn prepare_private_dir(path: &Path) -> Result<(), UpdateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(UpdateError::State(
                "update directory is not a regular directory".to_owned(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(state_error)?;
        }
        Err(error) => return Err(state_error(error)),
    }
    set_private_dir_permissions(path).map_err(state_error)
}

fn valid_etag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ETAG_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        && !value.contains(['\r', '\n'])
}

#[cfg(unix)]
fn set_private_open_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_open_mode(_: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), UpdateError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(state_error)
}

#[cfg(not(unix))]
fn sync_directory(_: &Path) -> Result<(), UpdateError> {
    Ok(())
}

fn state_error(error: impl std::fmt::Display) -> UpdateError {
    UpdateError::State(error.to_string())
}

#[cfg(test)]
mod tests;
