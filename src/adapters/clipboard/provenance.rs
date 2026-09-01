//! Private, generation-bound clipboard provenance state.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use fs4::{FileExt, TryLockError};
use serde::{Deserialize, Serialize};

const LOCK_ATTEMPTS: usize = 100;
const MAX_RECORD_BYTES: u64 = 4 * 1024;
const RECORD_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProvenanceRecord {
    schema_version: u8,
    pub(super) generation: u64,
    pub(super) request_id: String,
    pub(super) binding: [u8; 32],
}

impl ProvenanceRecord {
    pub(super) const fn new(generation: u64, request_id: String, binding: [u8; 32]) -> Self {
        Self {
            schema_version: RECORD_SCHEMA_VERSION,
            generation,
            request_id,
            binding,
        }
    }

    fn valid(&self) -> bool {
        self.schema_version == RECORD_SCHEMA_VERSION
            && self.request_id.parse::<crate::domain::RequestId>().is_ok()
    }
}

#[derive(Debug)]
pub(super) struct FileClipboardProvenance {
    root: PathBuf,
}

impl FileClipboardProvenance {
    pub(super) fn new(cache_directory: &Path) -> Self {
        Self {
            root: cache_directory.join("clipboard"),
        }
    }

    pub(super) fn acquire(&self) -> Result<ProvenanceLease, String> {
        if !self.root.is_absolute() {
            return Err("clipboard provenance directory must be absolute".to_owned());
        }
        crate::adapters::filesystem::prepare_private_dir(&self.root).map_err(io_error)?;
        let lock_path = self.root.join("provenance.lock");
        crate::adapters::filesystem::validate_file_path(&lock_path).map_err(io_error)?;
        let lock = open_private_file(&lock_path)?;
        for _ in 0..LOCK_ATTEMPTS {
            match FileExt::try_lock(&lock) {
                Ok(()) => {
                    return Ok(ProvenanceLease {
                        lock,
                        root: self.root.clone(),
                    });
                }
                Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(2)),
                Err(TryLockError::Error(error)) => return Err(io_error(error)),
            }
        }
        Err("clipboard provenance lock remained busy".to_owned())
    }
}

pub(super) struct ProvenanceLease {
    lock: File,
    root: PathBuf,
}

impl ProvenanceLease {
    pub(super) fn load(&self) -> Result<Option<ProvenanceRecord>, String> {
        let path = self.record_path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("clipboard provenance is not a regular file".to_owned());
        }
        if metadata.len() > MAX_RECORD_BYTES {
            return Ok(None);
        }
        set_private_file_permissions(&path).map_err(io_error)?;
        let bytes = fs::read(path).map_err(io_error)?;
        let Ok(record) = serde_json::from_slice::<ProvenanceRecord>(&bytes) else {
            return Ok(None);
        };
        Ok(record.valid().then_some(record))
    }

    pub(super) fn store(&self, record: &ProvenanceRecord) -> Result<(), String> {
        let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RECORD_BYTES {
            return Err("clipboard provenance exceeds its size limit".to_owned());
        }
        let destination = self.record_path();
        crate::adapters::filesystem::validate_file_path(&destination).map_err(io_error)?;
        let (temporary, mut file) = reserve_temporary(&self.root)?;
        let result = (|| {
            file.write_all(&bytes).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            fs::rename(&temporary, &destination).map_err(io_error)?;
            set_private_file_permissions(&destination).map_err(io_error)?;
            File::open(&self.root)
                .and_then(|directory| directory.sync_all())
                .map_err(io_error)
        })();
        if result.is_err() {
            let _removed = fs::remove_file(temporary);
        }
        result
    }

    fn record_path(&self) -> PathBuf {
        self.root.join("provenance.json")
    }
}

impl Drop for ProvenanceLease {
    fn drop(&mut self) {
        let _unlocked = FileExt::unlock(&self.lock);
    }
}

fn reserve_temporary(directory: &Path) -> Result<(PathBuf, File), String> {
    for suffix in 0..128_u16 {
        let path = directory.join(format!("provenance-{}-{suffix}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        set_private_open_mode(&mut options);
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Err("could not reserve atomic clipboard provenance state".to_owned())
}

fn open_private_file(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    set_private_open_mode(&mut options);
    let file = options.open(path).map_err(io_error)?;
    set_private_file_permissions(path).map_err(io_error)?;
    Ok(file)
}

fn set_private_open_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn io_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
