//! Private atomic installation-wide update state and operation locks.

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use fs4::{FileExt, TryLockError};

mod external;
mod lease;
mod persistence;
mod validation;

use lease::FileUpdateLease;
use validation::{classify_restart_completion, merge_latest, valid_loaded_state};

use crate::{
    domain::{
        ExternalRestartPending, InstallationIdentity, InstalledVersionRelation,
        ReleaseHighlightAnnouncement, SessionId, StableVersion, Timestamp, UpdateCacheState,
    },
    ports::update::{
        ExternalCacheTransition, ReleaseObservation, RestartCompletion, UpdateError, UpdateLease,
        UpdateLockKind, UpdateStateStore,
    },
};

const MAX_STATE_BYTES: u64 = 16 * 1024;
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
        self.write_atomic(&directory, &state_path, &state)?;
        drop(lock);
        Ok(state)
    }

    fn transition<T>(
        &self,
        installation: InstallationIdentity,
        change: impl FnOnce(&mut UpdateCacheState) -> Result<(T, bool), UpdateError>,
    ) -> Result<T, UpdateError> {
        let directory = self.installation_dir(installation)?;
        let lock = lock_state(&directory.join("state.lock"))?;
        let state_path = directory.join("state.json");
        let mut state = load_path(&state_path)?;
        let (result, changed) = change(&mut state)?;
        if changed {
            self.write_atomic(&directory, &state_path, &state)?;
        }
        drop(lock);
        Ok(result)
    }

    fn write_atomic(
        &self,
        directory: &Path,
        destination: &Path,
        state: &UpdateCacheState,
    ) -> Result<(), UpdateError> {
        if !directory.starts_with(&self.root) {
            return Err(UpdateError::State(
                "update state destination escaped its cache root".to_owned(),
            ));
        }
        #[cfg(test)]
        let fail_after_rename = fs::remove_file(self.root.join("test-fail-after-rename")).is_ok();
        #[cfg(not(test))]
        let fail_after_rename = false;
        persistence::write_atomic(directory, destination, state, fail_after_rename)
    }

    #[cfg(test)]
    pub(crate) fn fail_next_write_after_rename(&self) {
        fs::write(self.root.join("test-fail-after-rename"), []).expect("write test failure marker");
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
            UpdateLockKind::Convergence => "convergence.lock",
        };
        let file = open_private_file(&self.installation_dir(installation)?.join(name))?;
        match FileExt::try_lock(&file) {
            Ok(()) => Ok(Some(Box::new(FileUpdateLease { file }))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(state_error(error)),
        }
    }

    fn try_startup_lock(
        &self,
        installation: InstallationIdentity,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        let file = open_private_file(
            &self
                .installation_dir(installation)?
                .join("convergence.lock"),
        )?;
        match FileExt::try_lock_shared(&file) {
            Ok(()) => Ok(Some(Box::new(FileUpdateLease { file }))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(state_error(error)),
        }
    }

    fn wait_for_startup_lock(
        &self,
        installation: InstallationIdentity,
        timeout: Duration,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(lease) = self.try_startup_lock(installation)? {
                return Ok(Some(lease));
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(10).min(remaining));
        }
    }

    fn begin_refresh(
        &self,
        installation: InstallationIdentity,
        observed_generation: Option<u64>,
    ) -> Result<Option<UpdateCacheState>, UpdateError> {
        let mut advanced = false;
        let state = self.mutate(installation, |state| {
            if observed_generation.is_some_and(|generation| generation != state.refresh_generation)
            {
                return Ok(());
            }
            state.refresh_generation =
                state.refresh_generation.checked_add(1).ok_or_else(|| {
                    UpdateError::State("update refresh generation exhausted".to_owned())
                })?;
            advanced = true;
            Ok(())
        })?;
        Ok(advanced.then_some(state))
    }

    fn record_success(
        &self,
        installation: InstallationIdentity,
        observed: ReleaseObservation,
        installed: StableVersion,
        checked_at: Timestamp,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.mutate(installation, |state| {
            if state
                .external_restart
                .as_ref()
                .is_some_and(|pending| pending.target_version() != &installed)
            {
                return Err(UpdateError::State(
                    "update refresh conflicts with pending external restart".to_owned(),
                ));
            }
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
            match state.observed_installed_version.as_ref() {
                None => state.observed_installed_version = Some(installed),
                Some(observed)
                    if installed.relation_to_observed(observed)
                        == InstalledVersionRelation::Equal => {}
                Some(_stale_or_newer_authority) => {}
            }
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
            if state.external_restart.is_some() {
                return Err(UpdateError::State(
                    "in-app restart state conflicts with pending external restart".to_owned(),
                ));
            }
            state.observed_installed_version = Some(installed);
            state.restart_needed = restart_needed;
            Ok(())
        })
    }

    fn reconcile_external_upgrade(
        &self,
        installation: InstallationIdentity,
        expected_observed: &StableVersion,
        installed: &StableVersion,
        pending: Option<&ExternalRestartPending>,
    ) -> Result<ExternalCacheTransition, UpdateError> {
        external::reconcile(self, installation, expected_observed, installed, pending)
    }

    fn complete_external_restart(
        &self,
        installation: InstallationIdentity,
        installed: &StableVersion,
        pending: &ExternalRestartPending,
    ) -> Result<ExternalCacheTransition, UpdateError> {
        external::complete(self, installation, installed, pending)
    }

    fn acknowledge_external_resume(
        &self,
        installation: InstallationIdentity,
        installed: &StableVersion,
        pending: &ExternalRestartPending,
        session_id: SessionId,
    ) -> Result<ExternalCacheTransition, UpdateError> {
        external::acknowledge_resume(self, installation, installed, pending, session_id)
    }

    fn complete_restart(
        &self,
        installation: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<RestartCompletion, UpdateError> {
        let mut completion = RestartCompletion::Mismatch;
        self.mutate(installation, |state| {
            completion = classify_restart_completion(state, announcement);
            Ok(())
        })?;
        Ok(completion)
    }

    fn record_release_highlights(
        &self,
        installation: InstallationIdentity,
        announcement: ReleaseHighlightAnnouncement,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.mutate(installation, |state| {
            state.release_highlights = Some(announcement);
            Ok(())
        })
    }

    fn discard_release_highlights(
        &self,
        installation: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError> {
        let mut discarded = false;
        self.mutate(installation, |state| {
            let matches = state.release_highlights.as_ref().is_some_and(|current| {
                !current.acknowledged() && current.same_upgrade(announcement)
            });
            if matches {
                state.release_highlights = None;
                discarded = true;
            }
            Ok(())
        })?;
        Ok(discarded)
    }

    fn acknowledge_release_highlights(
        &self,
        installation: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError> {
        let mut acknowledged = false;
        self.mutate(installation, |state| {
            if let Some(current) = &mut state.release_highlights
                && !current.acknowledged()
                && current.same_upgrade(announcement)
            {
                current.acknowledge();
                acknowledged = true;
            }
            Ok(())
        })?;
        Ok(acknowledged)
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
    if !valid_loaded_state(&state) {
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

fn set_private_open_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

fn sync_directory(path: &Path) -> Result<(), UpdateError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(state_error)
}

fn state_error(error: impl std::fmt::Display) -> UpdateError {
    UpdateError::State(error.to_string())
}

#[cfg(test)]
mod tests;
