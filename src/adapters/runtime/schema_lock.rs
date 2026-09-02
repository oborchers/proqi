//! Bounded shared and exclusive schema locking.

use std::{
    fs::File,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use fs4::{FileExt, TryLockError};

use super::open_private_file;
use crate::ports::runtime::{Lease, RuntimeError};

const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_millis(5);

/// Bounded wait policy for shared and exclusive schema leases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaLockPolicy {
    wait_timeout: Duration,
    retry_interval: Duration,
}

impl SchemaLockPolicy {
    /// Construct a valid bounded wait policy.
    ///
    /// # Errors
    ///
    /// Returns a typed input error for zero durations or an interval longer than the timeout.
    pub fn new(wait_timeout: Duration, retry_interval: Duration) -> Result<Self, RuntimeError> {
        if wait_timeout.is_zero() || retry_interval.is_zero() || retry_interval > wait_timeout {
            return Err(RuntimeError::Invalid(
                "schema lock waits require a positive timeout and bounded retry interval"
                    .to_owned(),
            ));
        }
        Ok(Self {
            wait_timeout,
            retry_interval,
        })
    }
}

impl Default for SchemaLockPolicy {
    fn default() -> Self {
        Self {
            wait_timeout: DEFAULT_WAIT_TIMEOUT,
            retry_interval: DEFAULT_RETRY_INTERVAL,
        }
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

pub(super) fn acquire(
    path: &Path,
    exclusive: bool,
    policy: SchemaLockPolicy,
) -> Result<FileSchemaLease, RuntimeError> {
    let file = open_private_file(path)?;
    let deadline = Instant::now() + policy.wait_timeout;
    loop {
        let result = if exclusive {
            FileExt::try_lock(&file)
        } else {
            FileExt::try_lock_shared(&file)
        };
        match result {
            Ok(()) => return Ok(FileSchemaLease { file }),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                thread::sleep(policy.retry_interval.min(remaining));
            }
            Err(TryLockError::WouldBlock) => return Err(RuntimeError::SchemaBusy),
            Err(TryLockError::Error(error)) => return Err(super::io_error(error)),
        }
    }
}

pub(super) fn try_acquire(
    path: &Path,
    exclusive: bool,
) -> Result<Option<FileSchemaLease>, RuntimeError> {
    let file = open_private_file(path)?;
    let result = if exclusive {
        FileExt::try_lock(&file)
    } else {
        FileExt::try_lock_shared(&file)
    };
    match result {
        Ok(()) => Ok(Some(FileSchemaLease { file })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(super::io_error(error)),
    }
}
