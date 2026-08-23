//! Atomic private recovery exports.

use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use crate::{
    domain::RequestId,
    ports::recovery::{RecoveryDocument, RecoveryError, RecoveryExporter},
};

/// Filesystem-backed recovery writer rooted in Proqi's data directory.
pub struct FileRecoveryExporter {
    directory: PathBuf,
}

impl FileRecoveryExporter {
    /// Construct an exporter for one absolute directory.
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl RecoveryExporter for FileRecoveryExporter {
    fn export(
        &mut self,
        request_id: RequestId,
        document: &RecoveryDocument,
    ) -> Result<PathBuf, RecoveryError> {
        prepare_directory(&self.directory)?;
        let stem = format!("recovery-{}-{request_id}", document.session.id);
        let temporary = self.directory.join(format!(".{stem}.tmp"));
        let destination = self.directory.join(format!("{stem}.json"));
        refuse_existing(&temporary)?;
        refuse_existing(&destination)?;
        let result = write_and_install(document, &temporary, &destination, &self.directory);
        if result.is_err() {
            let _cleanup = fs::remove_file(&temporary);
        }
        result.map(|()| destination)
    }
}

fn prepare_directory(path: &Path) -> Result<(), RecoveryError> {
    if !path.is_absolute() {
        return Err(RecoveryError::InvalidDirectory(path.display().to_string()));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(RecoveryError::InvalidDirectory(path.display().to_string()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(io_error)?;
        }
        Err(error) => return Err(io_error(error)),
    }
    set_private_directory(path)
}

fn refuse_existing(path: &Path) -> Result<(), RecoveryError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(RecoveryError::Io(format!(
            "recovery target already exists: {}",
            path.display()
        ))),
        Err(error) => Err(io_error(error)),
    }
}

fn write_and_install(
    document: &RecoveryDocument,
    temporary: &Path,
    destination: &Path,
    directory: &Path,
) -> Result<(), RecoveryError> {
    let file = create_private_file(temporary)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, document)
        .map_err(|error| RecoveryError::Serialization(error.to_string()))?;
    writer.write_all(b"\n").map_err(io_error)?;
    writer.flush().map_err(io_error)?;
    writer.get_ref().sync_all().map_err(io_error)?;
    fs::rename(temporary, destination).map_err(io_error)?;
    sync_directory(directory)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), RecoveryError> {
    File::open(path)
        .and_then(|handle| handle.sync_all())
        .map_err(io_error)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), RecoveryError> {
    Ok(())
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<File, RecoveryError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(io_error)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<File, RecoveryError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<(), RecoveryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(io_error)
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> Result<(), RecoveryError> {
    Ok(())
}

fn io_error(error: std::io::Error) -> RecoveryError {
    let message = error.to_string();
    drop(error);
    RecoveryError::Io(message)
}
