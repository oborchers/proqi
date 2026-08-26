//! Size rotation and installation-wide inactive-log retention.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::SystemTime,
};

use fs4::{FileExt, TryLockError};
use tracing_subscriber::fmt::MakeWriter;

use crate::domain::InstanceId;

const MAX_SEGMENT_BYTES: u64 = 1024 * 1024;
const SEGMENTS_PER_INSTANCE: usize = 5;
const MAX_INSTALL_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct RotatingMakeWriter(Arc<Mutex<RotatingWriter>>);

struct RotatingWriter {
    base: PathBuf,
    file: File,
    bytes: u64,
    pending: Vec<u8>,
    _lock: File,
}

pub(super) struct LockedWriter<'a>(MutexGuard<'a, RotatingWriter>);

impl RotatingMakeWriter {
    pub(super) fn open(directory: &Path, instance_id: InstanceId) -> io::Result<Self> {
        create_private_dir(directory)?;
        let stem = instance_id.to_string();
        let lock = open_private(&directory.join(format!("{stem}.lock")), false)?;
        FileExt::try_lock(&lock).map_err(lock_error)?;
        prune_inactive(directory, &stem)?;
        let base = directory.join(format!("{stem}.jsonl"));
        let file = open_private(&base, false)?;
        let bytes = file.metadata()?.len();
        Ok(Self(Arc::new(Mutex::new(RotatingWriter {
            base,
            file,
            bytes,
            pending: Vec::new(),
            _lock: lock,
        }))))
    }
}

impl<'a> MakeWriter<'a> for RotatingMakeWriter {
    type Writer = LockedWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        let guard = match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        LockedWriter(guard)
    }
}

impl Write for LockedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.pending.extend_from_slice(bytes);
        while let Some(end) = self.0.pending.iter().position(|byte| *byte == b'\n') {
            let record = self.0.pending.drain(..=end).collect::<Vec<_>>();
            self.0.write_record(&record)?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.0.pending.is_empty() {
            let record = std::mem::take(&mut self.0.pending);
            self.0.write_record(&record)?;
        }
        self.0.file.flush()
    }
}

impl RotatingWriter {
    fn write_record(&mut self, record: &[u8]) -> io::Result<()> {
        let incoming = u64::try_from(record.len()).unwrap_or(u64::MAX);
        if self.bytes > 0 && self.bytes.saturating_add(incoming) > MAX_SEGMENT_BYTES {
            self.rotate()?;
        }
        self.file.write_all(record)?;
        self.file.flush()?;
        self.bytes = self.bytes.saturating_add(incoming);
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let oldest = segment_path(&self.base, SEGMENTS_PER_INSTANCE - 1);
        remove_if_exists(&oldest)?;
        for index in (1..SEGMENTS_PER_INSTANCE - 1).rev() {
            rename_if_exists(
                &segment_path(&self.base, index),
                &segment_path(&self.base, index + 1),
            )?;
        }
        rename_if_exists(&self.base, &segment_path(&self.base, 1))?;
        self.file = open_private(&self.base, true)?;
        self.bytes = 0;
        Ok(())
    }
}

fn prune_inactive(directory: &Path, current_stem: &str) -> io::Result<()> {
    let mut logs = diagnostic_logs(directory)?;
    let mut total = logs.iter().map(|entry| entry.bytes).sum::<u64>();
    logs.sort_by_key(|entry| entry.modified);
    for entry in logs {
        if total <= MAX_INSTALL_BYTES {
            break;
        }
        if entry.stem == current_stem || instance_is_active(directory, &entry.stem)? {
            continue;
        }
        remove_if_exists(&entry.path)?;
        total = total.saturating_sub(entry.bytes);
    }
    Ok(())
}

struct LogEntry {
    path: PathBuf,
    stem: String,
    bytes: u64,
    modified: SystemTime,
}

fn diagnostic_logs(directory: &Path) -> io::Result<Vec<LogEntry>> {
    let mut logs = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = diagnostic_stem(&name) else {
            continue;
        };
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            logs.push(LogEntry {
                path: entry.path(),
                stem: stem.to_owned(),
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
    Ok(logs)
}

fn diagnostic_stem(name: &str) -> Option<&str> {
    let (stem, suffix) = name.split_once(".jsonl")?;
    if stem.is_empty()
        || !(suffix.is_empty()
            || suffix
                .strip_prefix('.')
                .is_some_and(|index| index.parse::<usize>().is_ok()))
    {
        return None;
    }
    Some(stem)
}

fn instance_is_active(directory: &Path, stem: &str) -> io::Result<bool> {
    let lock = open_private(&directory.join(format!("{stem}.lock")), false)?;
    match FileExt::try_lock(&lock) {
        Ok(()) => {
            FileExt::unlock(&lock)?;
            Ok(false)
        }
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn segment_path(base: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{index}", base.display()))
}

fn rename_if_exists(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    crate::adapters::filesystem::prepare_private_dir(path)
}

fn open_private(path: &Path, truncate: bool) -> io::Result<File> {
    reject_symlink(path)?;
    let mut options = OpenOptions::new();
    options
        .create(true)
        .read(true)
        .write(true)
        .append(!truncate)
        .truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    make_file_private(&file, 0o600)?;
    Ok(file)
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostic path must not be a symbolic link",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn make_file_private(file: &File, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(mode))
}

fn lock_error(error: TryLockError) -> io::Error {
    match error {
        TryLockError::WouldBlock => {
            io::Error::new(io::ErrorKind::WouldBlock, "diagnostic log is active")
        }
        TryLockError::Error(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance_id() -> InstanceId {
        InstanceId::from_uuid(uuid::Uuid::now_v7()).expect("UUIDv7 instance")
    }

    #[test]
    fn records_rotate_at_a_mebibyte_and_retain_five_segments() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let writer = RotatingMakeWriter::open(temporary.path(), instance_id()).expect("writer");
        let mut record = vec![b'a'; 900 * 1024];
        record.push(b'\n');
        for _ in 0..6 {
            writer.make_writer().write_all(&record).expect("record");
        }
        let logs = diagnostic_logs(temporary.path()).expect("logs");
        assert_eq!(logs.len(), SEGMENTS_PER_INSTANCE);
        assert!(logs.iter().all(|entry| entry.bytes <= MAX_SEGMENT_BYTES));
    }

    #[test]
    fn installation_retention_prunes_old_inactive_logs() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        create_private_dir(temporary.path()).expect("directory");
        let payload = vec![b'a'; 1024 * 1024];
        for index in 0..22 {
            let path = temporary.path().join(format!("old-{index}.jsonl"));
            fs::write(path, &payload).expect("old log");
        }
        let _writer = RotatingMakeWriter::open(temporary.path(), instance_id()).expect("writer");
        let total = diagnostic_logs(temporary.path())
            .expect("logs")
            .iter()
            .map(|entry| entry.bytes)
            .sum::<u64>();
        assert!(total <= MAX_INSTALL_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_log_is_rejected() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().expect("temporary directory");
        let identifier = instance_id();
        let target = temporary.path().join("target");
        fs::write(&target, b"outside").expect("target");
        symlink(&target, temporary.path().join(format!("{identifier}.lock"))).expect("symlink");
        let error = RotatingMakeWriter::open(temporary.path(), identifier)
            .err()
            .expect("unsafe path rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
