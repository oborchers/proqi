//! Process coordination, leases, paths, and local control transport.

mod control_endpoint;
mod system;

pub use system::{NativePaths, SystemClock, SystemEnvironment, SystemIdGenerator};

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs4::{FileExt, TryLockError};

use crate::{
    domain::{InstanceId, SessionId, Timestamp},
    ports::{
        runtime::{
            InstanceInfo, Lease, RuntimeCoordinator, RuntimeError, RuntimeScan,
            UpdateInstanceContext,
        },
        store::STORAGE_PROTOCOL_VERSION,
    },
};

/// File-lock-backed runtime coordinator.
#[derive(Clone, Debug)]
pub struct FileRuntimeCoordinator {
    runtime_dir: PathBuf,
    instance_id: InstanceId,
    launch_directory: PathBuf,
    started_at: Timestamp,
    version: String,
    update: Option<UpdateInstanceContext>,
}

impl FileRuntimeCoordinator {
    /// Construct a process coordinator rooted in one user-only runtime directory.
    ///
    /// # Errors
    ///
    /// Returns an error when paths are relative or the runtime directory cannot be prepared.
    pub fn new(
        runtime_dir: PathBuf,
        instance_id: InstanceId,
        launch_directory: PathBuf,
        started_at: Timestamp,
        version: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        if !runtime_dir.is_absolute() || !launch_directory.is_absolute() {
            return Err(RuntimeError::Invalid(
                "runtime and launch directories must be absolute".to_owned(),
            ));
        }
        create_private_dir(&runtime_dir)?;
        create_private_dir(&runtime_dir.join("sessions"))?;
        create_private_dir(&runtime_dir.join("instances"))?;
        Ok(Self {
            runtime_dir,
            instance_id,
            launch_directory,
            started_at,
            version: version.into(),
            update: None,
        })
    }

    /// Advertise a verified installation and update-control protocol.
    #[must_use]
    pub fn with_update_context(
        mut self,
        installation_identity: crate::domain::InstallationIdentity,
        protocol: u32,
    ) -> Self {
        self.update = Some(UpdateInstanceContext {
            installation_identity,
            protocol,
        });
        self
    }

    fn session_lock_path(&self, session_id: SessionId) -> PathBuf {
        self.runtime_dir
            .join("sessions")
            .join(format!("{session_id}.lock"))
    }

    fn metadata_path(&self) -> PathBuf {
        self.runtime_dir
            .join("instances")
            .join(format!("{}.json", self.instance_id))
    }

    fn instance_info(&self, session_id: SessionId) -> InstanceInfo {
        InstanceInfo {
            instance_id: self.instance_id,
            session_id,
            pid: std::process::id(),
            version: self.version.clone(),
            storage_protocol: STORAGE_PROTOCOL_VERSION,
            control_protocol: None,
            control_endpoint: None,
            update: self.update.clone(),
            launch_directory: self.launch_directory.to_string_lossy().into_owned(),
            started_at: self.started_at,
        }
    }

    fn read_metadata(&self) -> Result<Vec<(PathBuf, InstanceInfo)>, RuntimeError> {
        let directory = self.runtime_dir.join("instances");
        let mut output = Vec::new();
        for entry in fs::read_dir(&directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(io_error(error)),
            };
            if let Ok(info) = serde_json::from_slice(&bytes) {
                output.push((path, info));
            }
        }
        Ok(output)
    }

    fn holder_for(&self, session_id: SessionId) -> Option<InstanceInfo> {
        self.read_metadata().ok().and_then(|entries| {
            entries
                .into_iter()
                .map(|(_, info)| info)
                .filter(|info| info.session_id == session_id)
                .max_by_key(|info| info.started_at)
        })
    }

    fn purge_metadata_for(&self, session_id: SessionId) -> Result<(), RuntimeError> {
        for (path, info) in self.read_metadata()? {
            if info.session_id == session_id {
                remove_stale_control_endpoint(&self.runtime_dir, &info)?;
                remove_if_exists(&path)?;
            }
        }
        Ok(())
    }
}

impl RuntimeCoordinator for FileRuntimeCoordinator {
    type SessionLease = FileSessionLease;
    type SharedSchemaLease = FileSchemaLease;
    type ExclusiveSchemaLease = FileSchemaLease;

    fn acquire_session(&self, session_id: SessionId) -> Result<Self::SessionLease, RuntimeError> {
        let lock_path = self.session_lock_path(session_id);
        let file = open_private_file(&lock_path)?;
        match try_session_lock(&file) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(RuntimeError::SessionBusy {
                    session_id,
                    holder: self.holder_for(session_id).map(Box::new),
                });
            }
            Err(TryLockError::Error(error)) => return Err(io_error(error)),
        }
        if let Err(error) = self.purge_metadata_for(session_id) {
            let _ = FileExt::unlock(&file);
            return Err(error);
        }
        let metadata_path = self.metadata_path();
        let info = self.instance_info(session_id);
        if let Err(error) = write_private_json(&metadata_path, &info) {
            let _ = FileExt::unlock(&file);
            return Err(error);
        }
        Ok(FileSessionLease {
            file,
            metadata_path,
            info,
            prepared_control_endpoint: control_endpoint::prepare(
                &self.runtime_dir,
                self.instance_id,
            )
            .ok()
            .flatten(),
        })
    }

    fn acquire_schema_shared(&self) -> Result<Self::SharedSchemaLease, RuntimeError> {
        schema_lease(&self.runtime_dir.join("schema.lock"), false)
    }

    fn acquire_schema_exclusive(&self) -> Result<Self::ExclusiveSchemaLease, RuntimeError> {
        schema_lease(&self.runtime_dir.join("schema.lock"), true)
    }

    fn scan_runtime(&self) -> Result<RuntimeScan, RuntimeError> {
        let entries = self.read_metadata()?;
        let mut active = BTreeMap::new();
        let mut recovered = std::collections::BTreeSet::new();
        for (path, info) in entries {
            let file = open_private_file(&self.session_lock_path(info.session_id))?;
            match FileExt::try_lock_shared(&file) {
                Ok(()) => {
                    FileExt::unlock(&file).map_err(io_error)?;
                    remove_stale_control_endpoint(&self.runtime_dir, &info)?;
                    remove_if_exists(&path)?;
                    recovered.insert(info.session_id);
                }
                Err(TryLockError::WouldBlock) => {
                    retain_latest_instance(&mut active, info);
                }
                Err(TryLockError::Error(error)) => return Err(io_error(error)),
            }
        }
        let mut active: Vec<_> = active.into_values().collect();
        active.sort_by_key(|info| (info.started_at, info.instance_id));
        Ok(RuntimeScan {
            active,
            recovered: recovered.into_iter().collect(),
        })
    }
}

impl crate::ports::update::UpdateInstanceRegistry for FileRuntimeCoordinator {
    fn active_instances(&self) -> Result<Vec<InstanceInfo>, crate::ports::update::UpdateError> {
        RuntimeCoordinator::active_instances(self)
            .map_err(|error| crate::ports::update::UpdateError::Coordination(error.to_string()))
    }
}

fn remove_stale_control_endpoint(
    runtime_dir: &Path,
    info: &InstanceInfo,
) -> Result<(), RuntimeError> {
    let Some(endpoint) = info.control_endpoint.as_deref() else {
        return Ok(());
    };
    let Some(expected) = control_endpoint::existing(runtime_dir, info.instance_id)? else {
        return Ok(());
    };
    if Path::new(endpoint) == expected {
        remove_if_exists(Path::new(endpoint))?;
    }
    Ok(())
}

fn try_session_lock(file: &File) -> Result<(), TryLockError> {
    match FileExt::try_lock(file) {
        Err(TryLockError::WouldBlock) => {
            std::thread::sleep(std::time::Duration::from_millis(2));
            FileExt::try_lock(file)
        }
        result => result,
    }
}

fn retain_latest_instance(active: &mut BTreeMap<SessionId, InstanceInfo>, info: InstanceInfo) {
    let replace = active
        .get(&info.session_id)
        .is_none_or(|existing| existing.started_at < info.started_at);
    if replace {
        active.insert(info.session_id, info);
    }
}

/// Authoritative session lease released on drop or process exit.
#[derive(Debug)]
pub struct FileSessionLease {
    file: File,
    metadata_path: PathBuf,
    info: InstanceInfo,
    prepared_control_endpoint: Option<String>,
}

impl FileSessionLease {
    /// Descriptive information corresponding to this lease.
    #[must_use]
    pub const fn info(&self) -> &InstanceInfo {
        &self.info
    }

    /// Return the verified endpoint reserved for an interactive owner.
    #[must_use]
    pub fn control_endpoint(&self) -> Option<&str> {
        self.prepared_control_endpoint.as_deref()
    }

    /// Publish control capability after the listener has bound successfully.
    ///
    /// # Errors
    ///
    /// Returns a typed metadata write failure.
    pub fn publish_control(&mut self) -> Result<(), RuntimeError> {
        let endpoint = self
            .prepared_control_endpoint
            .clone()
            .ok_or_else(|| RuntimeError::Invalid("local control is unsupported".to_owned()))?;
        self.info.control_protocol = Some(crate::ports::control::CONTROL_PROTOCOL_VERSION);
        self.info.control_endpoint = Some(endpoint);
        replace_private_json(&self.metadata_path, &self.info)
    }
}

impl Lease for FileSessionLease {}

impl Drop for FileSessionLease {
    fn drop(&mut self) {
        if let Some(endpoint) = &self.prepared_control_endpoint {
            let _ = remove_if_exists(Path::new(endpoint));
        }
        let _ = remove_if_exists(&self.metadata_path);
        let _ = FileExt::unlock(&self.file);
    }
}

/// Shared or exclusive schema lease released automatically.
#[derive(Debug)]
pub struct FileSchemaLease {
    file: File,
}

impl Lease for FileSchemaLease {}

impl Drop for FileSchemaLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn schema_lease(path: &Path, exclusive: bool) -> Result<FileSchemaLease, RuntimeError> {
    let file = open_private_file(path)?;
    let result = if exclusive {
        FileExt::try_lock(&file)
    } else {
        FileExt::try_lock_shared(&file)
    };
    match result {
        Ok(()) => Ok(FileSchemaLease { file }),
        Err(TryLockError::WouldBlock) => Err(RuntimeError::SchemaBusy),
        Err(TryLockError::Error(error)) => Err(io_error(error)),
    }
}

fn write_private_json(path: &Path, value: &InstanceInfo) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| RuntimeError::MalformedMetadata(error.to_string()))?;
    let mut file = create_new_private_file(path)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn replace_private_json(path: &Path, value: &InstanceInfo) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| RuntimeError::MalformedMetadata(error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    remove_if_exists(&temporary)?;
    let mut file = create_new_private_file(&temporary)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)
}

fn create_private_dir(path: &Path) -> Result<(), RuntimeError> {
    fs::create_dir_all(path).map_err(io_error)?;
    set_private_dir_permissions(path).map_err(io_error)
}

fn open_private_file(path: &Path) -> Result<File, RuntimeError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    set_private_file_mode(&mut options);
    let file = options.open(path).map_err(io_error)?;
    set_private_file_permissions(path).map_err(io_error)?;
    Ok(file)
}

fn create_new_private_file(path: &Path) -> Result<File, RuntimeError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    set_private_file_mode(&mut options);
    options.open(path).map_err(io_error)
}

fn remove_if_exists(path: &Path) -> Result<(), RuntimeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "I/O errors are consumed and converted at adapter boundaries"
)]
fn io_error(error: std::io::Error) -> RuntimeError {
    RuntimeError::Io(error.to_string())
}

#[cfg(unix)]
fn set_private_file_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_file_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
