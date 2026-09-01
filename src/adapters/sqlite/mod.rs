//! SQLite persistence adapter.

mod board_commit;
mod capture;
mod compaction;
mod doctor;
mod history_commit;
mod load;
mod migration;
mod onboarding;
mod operation_lookup;
mod receipt_compaction;
mod schema;
mod search;
mod session_admin;
mod submission;
mod support;

#[cfg(test)]
mod tests;

pub use doctor::{SqliteHealth, inspect_read_only_snapshot};

use std::{path::PathBuf, thread, time::Duration};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};

use crate::{
    domain::{SessionId, Timestamp},
    ports::store::{
        CaptureCommit, CaptureCommitOutcome, CommitReceipt, FirstRunBoard, FirstRunOutcome,
        MigrationMode, OperationBatch, STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION,
        SessionHit, SessionQuery, SessionSnapshot, Store, StoreError, StoredOperationRequest,
        SubmissionAttempt, SubmissionOutcome,
    },
};

use self::{
    board_commit::commit_batch,
    load::load_snapshot,
    migration::{
        create_backup, migrate, quick_check, retry_delay, schema_version, storage_protocol,
    },
    search::{load_hit, rebuild_session_search, search_ids},
    support::{
        create_private_dir, create_private_file, map_sql_error, session_id_from_blob,
        set_private_file_permissions, set_sqlite_permissions,
    },
};

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

/// Bundled SQLite store.
pub struct SqliteStore {
    connection: Connection,
    database_path: PathBuf,
    retry: RetryPolicy,
}

#[cfg(test)]
pub(crate) use support::TestWriteLock;

impl SqliteStore {
    #[cfg(test)]
    pub(crate) fn acquire_test_write_lock(&self) -> Result<TestWriteLock, StoreError> {
        let connection = Connection::open(&self.database_path).map_err(map_sql_error)?;
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(map_sql_error)?;
        Ok(TestWriteLock(connection))
    }

    /// Open, validate, and when authorized migrate one local database.
    ///
    /// # Errors
    ///
    /// Returns a typed path, SQLite, integrity, backup, or compatibility failure.
    pub fn open(config: &StoreConfig) -> Result<Self, StoreError> {
        if !config.database_path.is_absolute() || !config.backup_dir.is_absolute() {
            return Err(StoreError::Io(
                "database and backup paths must be absolute".to_owned(),
            ));
        }
        let parent = config
            .database_path
            .parent()
            .ok_or_else(|| StoreError::Io("database path has no parent".to_owned()))?;
        create_private_dir(parent)?;
        validate_sqlite_paths(&config.database_path)?;
        let existed_with_content = config
            .database_path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.len() > 0);
        if config.database_path.symlink_metadata().is_err() {
            create_private_file(&config.database_path)?;
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        let mut connection =
            Connection::open_with_flags(&config.database_path, flags).map_err(map_sql_error)?;
        set_private_file_permissions(&config.database_path)?;
        connection
            .busy_timeout(config.retry.busy_timeout)
            .map_err(map_sql_error)?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(map_sql_error)?;

        let found = schema_version(&connection)?;
        if found > SUPPORTED_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema {
                found,
                supported: SUPPORTED_SCHEMA_VERSION,
            });
        }
        if found != 0 {
            let protocol = storage_protocol(&connection)?;
            if protocol > STORAGE_PROTOCOL_VERSION {
                return Err(StoreError::UnsupportedStorageProtocol {
                    found: protocol,
                    supported: STORAGE_PROTOCOL_VERSION,
                });
            }
        }
        if found < SUPPORTED_SCHEMA_VERSION {
            if config.migration_mode == MigrationMode::Refuse {
                return Err(StoreError::MigrationRequired {
                    found,
                    supported: SUPPORTED_SCHEMA_VERSION,
                });
            }
            if existed_with_content {
                create_backup(&connection, config, found)?;
                quick_check(&connection)?;
            }
            migrate(&mut connection, found, config.migration_time)?;
            quick_check(&connection)?;
        }

        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(map_sql_error)?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(map_sql_error)?;
        set_sqlite_permissions(&config.database_path)?;
        Ok(Self {
            connection,
            database_path: config.database_path.clone(),
            retry: config.retry,
        })
    }

    /// Current SQLite journal mode, used by diagnostics and contract tests.
    ///
    /// # Errors
    ///
    /// Returns a typed SQLite failure.
    pub fn journal_mode(&self) -> Result<String, StoreError> {
        self.connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .map_err(map_sql_error)
    }

    /// Current SQLite synchronous level, where `2` means `FULL`.
    ///
    /// # Errors
    ///
    /// Returns a typed SQLite failure.
    pub fn synchronous_level(&self) -> Result<i64, StoreError> {
        self.connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .map_err(map_sql_error)
    }

    /// Validate canonical tables with SQLite's quick check.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Integrity`] unless SQLite reports exactly `ok`.
    pub fn quick_check(&self) -> Result<(), StoreError> {
        quick_check(&self.connection)
    }

    /// Rebuild the derived full-text index from canonical rows.
    ///
    /// # Errors
    ///
    /// Returns a typed SQLite or corruption failure.
    pub fn rebuild_search_index(&mut self) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| {
            transaction
                .execute("DELETE FROM session_search", [])
                .map_err(map_sql_error)?;
            let mut statement = transaction
                .prepare("SELECT id FROM sessions")
                .map_err(map_sql_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, Vec<u8>>(0))
                .map_err(map_sql_error)?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(session_id_from_blob(row.map_err(map_sql_error)?)?);
            }
            drop(statement);
            for id in ids {
                rebuild_session_search(transaction, id)?;
            }
            Ok(())
        })
    }

    fn with_write_retry<T>(
        &mut self,
        mut operation: impl FnMut(&Transaction<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let attempts = self.retry.max_attempts.max(1);
        let database_path = self.database_path.clone();
        for attempt in 0..attempts {
            let result = (|| {
                let transaction = self
                    .connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(map_sql_error)?;
                let value = operation(&transaction)?;
                set_sqlite_permissions(&database_path)?;
                transaction.commit().map_err(map_sql_error)?;
                Ok(value)
            })();
            match result {
                Err(StoreError::Busy) if attempt + 1 < attempts => {
                    thread::sleep(retry_delay(
                        self.retry.base_delay,
                        attempt,
                        self.retry.jitter_seed,
                    ));
                }
                other => return other,
            }
        }
        Err(StoreError::Busy)
    }
}

fn validate_sqlite_paths(database_path: &std::path::Path) -> Result<(), StoreError> {
    support::validate_file_path(database_path)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut companion = database_path.as_os_str().to_os_string();
        companion.push(suffix);
        support::validate_file_path(std::path::Path::new(&companion))?;
    }
    Ok(())
}

impl Store for SqliteStore {
    fn load_session(&mut self, id: SessionId) -> Result<SessionSnapshot, StoreError> {
        load_snapshot(&self.connection, id)
    }

    fn compact_session(&mut self, id: SessionId) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| compaction::compact_session(transaction, id))
    }

    fn search_sessions(&mut self, query: &SessionQuery) -> Result<Vec<SessionHit>, StoreError> {
        let ids = search_ids(&self.connection, query)?;
        let mut hits = Vec::with_capacity(ids.len());
        for id in ids {
            hits.push(load_hit(&self.connection, id)?);
        }
        hits.sort_by(|left, right| {
            let left_current = query
                .current_directory
                .as_ref()
                .is_some_and(|path| path == &left.last_opened_cwd);
            let right_current = query
                .current_directory
                .as_ref()
                .is_some_and(|path| path == &right.last_opened_cwd);
            right_current
                .cmp(&left_current)
                .then_with(|| right.last_active_at.cmp(&left.last_active_at))
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(hits)
    }

    fn record_session_open(
        &mut self,
        id: SessionId,
        cwd: &std::path::Path,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| session_admin::record_open(transaction, id, cwd, at))
    }

    fn rename_session(&mut self, id: SessionId, name: Option<&str>) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| session_admin::rename(transaction, id, name))
    }

    fn operation_request(
        &mut self,
        id: crate::domain::OperationId,
    ) -> Result<Option<StoredOperationRequest>, StoreError> {
        operation_lookup::operation_request(&self.connection, id)
    }

    fn revision_request(
        &mut self,
        id: crate::domain::RevisionId,
    ) -> Result<Option<StoredOperationRequest>, StoreError> {
        operation_lookup::revision_request(&self.connection, id)
    }

    fn create_first_run_session(
        &mut self,
        board: &FirstRunBoard,
    ) -> Result<FirstRunOutcome, StoreError> {
        self.with_write_retry(|transaction| onboarding::create(transaction, board))
    }

    fn commit(&mut self, batch: &OperationBatch) -> Result<Option<CommitReceipt>, StoreError> {
        self.with_write_retry(|transaction| commit_batch(transaction, batch))
    }

    fn commit_capture(
        &mut self,
        capture: &CaptureCommit,
    ) -> Result<CaptureCommitOutcome, StoreError> {
        self.with_write_retry(|transaction| capture::commit(transaction, capture))
    }

    fn prepare_submission(&mut self, attempt: &SubmissionAttempt) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| submission::prepare(transaction, attempt))
    }

    fn mark_submission_sending(
        &mut self,
        id: crate::domain::SubmissionId,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| submission::mark_sending(transaction, id, at))
    }

    fn finish_submission(
        &mut self,
        id: crate::domain::SubmissionId,
        outcome: &SubmissionOutcome,
    ) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| submission::finish(transaction, id, outcome))
    }

    fn finish_submission_with_removal(
        &mut self,
        id: crate::domain::SubmissionId,
        outcome: &SubmissionOutcome,
        removal: &crate::domain::BoardOperation,
    ) -> Result<CommitReceipt, StoreError> {
        self.with_write_retry(|transaction| {
            submission::finish_with_removal(transaction, id, outcome, removal)
        })
    }

    fn recover_submissions(
        &mut self,
        session_id: SessionId,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| submission::recover(transaction, session_id, at))
    }

    fn trash_session(&mut self, id: SessionId, at: Timestamp) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE sessions SET deleted_at = ?2, last_active_at = max(last_active_at, ?2) WHERE id = ?1",
                    params![id.database_bytes().as_slice(), at.as_millis()],
                )
                .map_err(map_sql_error)?;
            if changed == 0 {
                return Err(StoreError::NotFound(id.to_string()));
            }
            rebuild_session_search(transaction, id)
        })
    }

    fn restore_session(&mut self, id: SessionId) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE sessions SET deleted_at = NULL WHERE id = ?1",
                    [id.database_bytes().as_slice()],
                )
                .map_err(map_sql_error)?;
            if changed == 0 {
                return Err(StoreError::NotFound(id.to_string()));
            }
            rebuild_session_search(transaction, id)
        })
    }

    fn prune_session(&mut self, id: SessionId) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| {
            let deleted: Option<Option<i64>> = transaction
                .query_row(
                    "SELECT deleted_at FROM sessions WHERE id = ?1",
                    [id.database_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_sql_error)?;
            match deleted {
                None => return Err(StoreError::NotFound(id.to_string())),
                Some(None) => {
                    return Err(StoreError::Conflict(
                        "live sessions must be trashed before pruning".to_owned(),
                    ));
                }
                Some(Some(_)) => {}
            }
            transaction
                .execute(
                    "DELETE FROM session_search WHERE session_id = ?1",
                    [id.to_string()],
                )
                .map_err(map_sql_error)?;
            transaction
                .execute(
                    "DELETE FROM sessions WHERE id = ?1",
                    [id.database_bytes().as_slice()],
                )
                .map_err(map_sql_error)?;
            Ok(())
        })
    }
}
