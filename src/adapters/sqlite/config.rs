//! SQLite opening, migration and bounded retry configuration.

use crate::{domain::Timestamp, ports::store::MigrationMode};
use std::{path::PathBuf, time::Duration};

/// Bounded SQLite contention policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    /// Busy timeout applied inside SQLite.
    pub busy_timeout: Duration,
    /// Total transaction attempts, including the initial attempt.
    pub max_attempts: u32,
    /// Base delay used by deterministic bounded jitter.
    pub base_delay: Duration,
    /// Per-process seed used to vary retries between competing processes.
    pub jitter_seed: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        let seed = u64::from(std::process::id())
            ^ u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos()),
            )
            .unwrap_or(u64::MAX);
        Self {
            busy_timeout: Duration::from_millis(250),
            max_attempts: 4,
            base_delay: Duration::from_millis(8),
            jitter_seed: seed,
        }
    }
}

/// SQLite opening and migration configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreConfig {
    /// Canonical database path.
    pub database_path: PathBuf,
    /// Directory retaining pre-migration backups.
    pub backup_dir: PathBuf,
    /// Whether the caller holds exclusive migration authority.
    pub migration_mode: MigrationMode,
    /// Timestamp recorded for migrations and backup names.
    pub migration_time: Timestamp,
    /// Bounded contention behavior.
    pub retry: RetryPolicy,
}

impl StoreConfig {
    /// Construct configuration with the production retry policy.
    #[must_use]
    pub fn new(
        database_path: PathBuf,
        backup_dir: PathBuf,
        migration_mode: MigrationMode,
        migration_time: Timestamp,
    ) -> Self {
        Self {
            database_path,
            backup_dir,
            migration_mode,
            migration_time,
            retry: RetryPolicy::default(),
        }
    }
}
