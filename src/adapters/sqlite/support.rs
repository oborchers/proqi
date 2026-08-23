//! SQLite codecs, error mapping, persistence helpers, and private permissions.

use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, ErrorCode};

use crate::{
    domain::{OperationId, OperationSequence, RevisionId, SessionId, ThoughtId},
    ports::store::StoreError,
};

pub(super) fn validate_commit_sequence(
    connection: &Connection,
    session_id: SessionId,
    durable: OperationSequence,
) -> Result<(), StoreError> {
    let (count, minimum, maximum): (i64, Option<i64>, Option<i64>) = connection
        .query_row(
            "SELECT count(*), min(sequence), max(sequence) FROM commit_receipts WHERE session_id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(map_sql_error)?;
    let durable = sequence_to_i64(durable)?;
    let valid = if durable == 0 {
        count == 0 && minimum.is_none() && maximum.is_none()
    } else {
        count == durable && minimum == Some(1) && maximum == Some(durable)
    };
    if valid {
        Ok(())
    } else {
        Err(StoreError::Corrupt(
            "commit receipt sequence is not contiguous".to_owned(),
        ))
    }
}

pub(super) fn map_sql_error(error: rusqlite::Error) -> StoreError {
    match error {
        rusqlite::Error::SqliteFailure(code, message) => match code.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => StoreError::Busy,
            ErrorCode::DiskFull => StoreError::DiskFull,
            ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                StoreError::Corrupt(message.unwrap_or_else(|| code.to_string()))
            }
            ErrorCode::ConstraintViolation => {
                StoreError::Conflict(message.unwrap_or_else(|| code.to_string()))
            }
            _ => StoreError::Io(message.unwrap_or_else(|| code.to_string())),
        },
        other => StoreError::Io(other.to_string()),
    }
}

pub(super) fn bool_from_i64(value: i64) -> Result<bool, StoreError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StoreError::Corrupt("invalid SQLite boolean".to_owned())),
    }
}

pub(super) fn sequence_to_i64(sequence: OperationSequence) -> Result<i64, StoreError> {
    i64::try_from(sequence.get())
        .map_err(|_| StoreError::Corrupt("operation sequence exceeds SQLite range".to_owned()))
}

pub(super) fn sequence_from_i64(value: i64) -> Result<OperationSequence, StoreError> {
    u64::try_from(value)
        .map(OperationSequence::new)
        .map_err(|_| StoreError::Corrupt("negative operation sequence".to_owned()))
}

pub(super) fn usize_to_i64(value: usize) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Corrupt("index exceeds SQLite range".to_owned()))
}

pub(super) fn i64_to_usize(value: i64) -> Result<usize, StoreError> {
    usize::try_from(value)
        .map_err(|_| StoreError::Corrupt("negative or oversized index".to_owned()))
}

pub(super) fn i64_to_u32(value: i64) -> Result<u32, StoreError> {
    u32::try_from(value)
        .map_err(|_| StoreError::Corrupt("thought position exceeds valid range".to_owned()))
}

macro_rules! id_from_blob {
    ($function:ident, $type:ty) => {
        pub(super) fn $function(bytes: Vec<u8>) -> Result<$type, StoreError> {
            let array: [u8; 16] = bytes
                .try_into()
                .map_err(|_| StoreError::Corrupt("identifier BLOB is not 16 bytes".to_owned()))?;
            <$type>::from_database_bytes(array)
                .map_err(|error| StoreError::Corrupt(error.to_string()))
        }
    };
}

id_from_blob!(session_id_from_blob, SessionId);
id_from_blob!(thought_id_from_blob, ThoughtId);
id_from_blob!(operation_id_from_blob, OperationId);
id_from_blob!(revision_id_from_blob, RevisionId);

#[cfg(unix)]
pub(super) fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
pub(super) fn path_from_bytes(bytes: Vec<u8>) -> Result<PathBuf, StoreError> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(windows)]
pub(super) fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(windows)]
pub(super) fn path_from_bytes(bytes: Vec<u8>) -> Result<PathBuf, StoreError> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    if !bytes.len().is_multiple_of(2) {
        return Err(StoreError::Corrupt(
            "Windows path BLOB has odd length".to_owned(),
        ));
    }
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    Ok(PathBuf::from(OsString::from_wide(&wide)))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn path_to_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}

#[cfg(not(any(unix, windows)))]
pub(super) fn path_from_bytes(bytes: Vec<u8>) -> Result<PathBuf, StoreError> {
    String::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|error| StoreError::Corrupt(error.to_string()))
}

pub(super) fn create_private_dir(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path).map_err(|error| StoreError::Io(error.to_string()))?;
    set_private_dir_permissions(path)
}

pub(super) fn create_private_file(path: &Path) -> Result<(), StoreError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    set_private_open_mode(&mut options);
    options
        .open(path)
        .map(|_| ())
        .map_err(|error| StoreError::Io(error.to_string()))
}

pub(super) fn set_sqlite_permissions(database_path: &Path) -> Result<(), StoreError> {
    set_private_file_permissions(database_path)?;
    for suffix in ["-wal", "-shm"] {
        let companion = companion_path(database_path, suffix);
        if companion.exists() {
            set_private_file_permissions(&companion)?;
        }
    }
    Ok(())
}

fn companion_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut path = database_path.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

#[cfg(unix)]
pub(super) fn set_private_open_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
pub(super) fn set_private_open_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
pub(super) fn set_private_file_permissions(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| StoreError::Io(error.to_string()))
}

#[cfg(not(unix))]
pub(super) fn set_private_file_permissions(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| StoreError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), StoreError> {
    Ok(())
}
