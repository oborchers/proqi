use thiserror::Error;

/// Typed persistence failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StoreError {
    /// SQLite writer remained contended after bounded retries.
    #[error("storage is busy")]
    Busy,
    /// A requested durable record does not exist.
    #[error("storage record not found: {0}")]
    NotFound(String),
    /// Current state does not satisfy a commit precondition.
    #[error("storage conflict: {0}")]
    Conflict(String),
    /// Database or index integrity validation failed.
    #[error("storage integrity check failed: {0}")]
    Integrity(String),
    /// Database schema is newer than this binary.
    #[error("unsupported storage schema {found}, maximum supported is {supported}")]
    UnsupportedSchema {
        /// Schema found on disk.
        found: u32,
        /// Maximum supported schema.
        supported: u32,
    },
    /// Storage protocol is newer even though the table schema is recognized.
    #[error("unsupported storage protocol {found}, maximum supported is {supported}")]
    UnsupportedStorageProtocol {
        /// Protocol found on disk.
        found: u32,
        /// Maximum supported protocol.
        supported: u32,
    },
    /// Schema is older but this process lacks exclusive migration authority.
    #[error("storage schema {found} requires migration to {supported}")]
    MigrationRequired {
        /// Schema found on disk.
        found: u32,
        /// Required schema.
        supported: u32,
    },
    /// A pre-migration backup could not be completed.
    #[error("storage backup failed: {0}")]
    Backup(String),
    /// Database contents or identifiers are malformed.
    #[error("storage is corrupt or malformed: {0}")]
    Corrupt(String),
    /// Filesystem operation failed.
    #[error("storage I/O failed: {0}")]
    Io(String),
    /// JSON operation payload could not be encoded or decoded.
    #[error("storage serialization failed: {0}")]
    Serialization(String),
    /// A validated domain invariant rejected persisted state.
    #[error("stored domain invariant failed: {0}")]
    Invariant(String),
    /// Available storage could not accept a write.
    #[error("storage device is full")]
    DiskFull,
    /// The in-memory recovery queue cannot safely retain another failed write.
    #[error("failed write exceeds bounded recovery capacity")]
    RecoveryCapacity,
}

/// Stable semantic categories translated at each existing transport boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreFailureCode {
    /// Storage writer contention exceeded its bounded retry policy.
    Busy,
    /// Requested durable state does not exist.
    NotFound,
    /// Durable preconditions conflict with current state.
    Conflict,
    /// The installed binary cannot safely operate on this storage version.
    Unsupported,
    /// The storage device cannot accept another write.
    DiskFull,
    /// Failed work cannot fit in the bounded recovery queue.
    RecoveryCapacity,
    /// Any other persistence failure.
    Failed,
}

impl StoreFailureCode {
    /// Stable machine-readable spelling used by local CLI responses.
    #[must_use]
    pub const fn cli_str(self) -> &'static str {
        match self {
            Self::Busy => "storage_busy",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Unsupported => "unsupported",
            Self::DiskFull => "disk_full",
            Self::RecoveryCapacity => "recovery_capacity",
            Self::Failed => "storage_failed",
        }
    }

    /// Stable spelling used by the current owner-control protocol.
    #[must_use]
    pub const fn control_str(self) -> &'static str {
        match self {
            Self::Busy => "storage_busy",
            Self::DiskFull => "storage_full",
            Self::RecoveryCapacity => "recovery_capacity",
            Self::NotFound | Self::Conflict | Self::Unsupported | Self::Failed => "storage_failed",
        }
    }
}

impl StoreError {
    /// Stable storage-failure category independent of the calling transport.
    #[must_use]
    pub const fn failure_code(&self) -> StoreFailureCode {
        match self {
            Self::Busy => StoreFailureCode::Busy,
            Self::NotFound(_) => StoreFailureCode::NotFound,
            Self::Conflict(_) => StoreFailureCode::Conflict,
            Self::UnsupportedSchema { .. }
            | Self::UnsupportedStorageProtocol { .. }
            | Self::MigrationRequired { .. } => StoreFailureCode::Unsupported,
            Self::DiskFull => StoreFailureCode::DiskFull,
            Self::RecoveryCapacity => StoreFailureCode::RecoveryCapacity,
            Self::Integrity(_)
            | Self::Backup(_)
            | Self::Corrupt(_)
            | Self::Io(_)
            | Self::Serialization(_)
            | Self::Invariant(_) => StoreFailureCode::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StoreError, StoreFailureCode};

    #[test]
    fn typed_categories_preserve_existing_cli_and_control_spellings() {
        assert_eq!(StoreFailureCode::DiskFull.cli_str(), "disk_full");
        assert_eq!(StoreFailureCode::DiskFull.control_str(), "storage_full");
        assert_eq!(
            StoreError::Busy.failure_code().control_str(),
            "storage_busy"
        );
        assert_eq!(
            StoreError::Conflict("stale".to_owned())
                .failure_code()
                .control_str(),
            "storage_failed"
        );
    }
}
