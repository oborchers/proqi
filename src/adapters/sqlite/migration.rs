//! Forward-only schema migration, backup, and integrity helpers.

use std::{
    fs::{self, OpenOptions},
    path::PathBuf,
    time::Duration,
};

use rusqlite::{Connection, TransactionBehavior, params};

use crate::{
    domain::Timestamp,
    ports::store::{STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION, StoreError},
};

use super::{
    StoreConfig,
    schema::{
        MIGRATION_1, MIGRATION_2, MIGRATION_3, MIGRATION_4, MIGRATION_5, MIGRATION_6, MIGRATION_7,
    },
    support::{
        create_private_dir, map_sql_error, set_private_file_permissions, set_private_open_mode,
    },
};

pub(super) fn schema_version(connection: &Connection) -> Result<u32, StoreError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if !exists {
        return Ok(0);
    }
    let version: i64 = connection
        .query_row(
            "SELECT schema_version FROM schema_meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    u32::try_from(version).map_err(|_| StoreError::Corrupt("invalid schema version".to_owned()))
}

pub(super) fn storage_protocol(connection: &Connection) -> Result<u32, StoreError> {
    let protocol: i64 = connection
        .query_row(
            "SELECT storage_protocol FROM schema_meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    u32::try_from(protocol).map_err(|_| StoreError::Corrupt("invalid storage protocol".to_owned()))
}

pub(super) fn migrate(
    connection: &mut Connection,
    found: u32,
    at: Timestamp,
) -> Result<(), StoreError> {
    if found >= SUPPORTED_SCHEMA_VERSION {
        return Err(StoreError::MigrationRequired {
            found,
            supported: SUPPORTED_SCHEMA_VERSION,
        });
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .map_err(map_sql_error)?;
    match found {
        0 => transaction.execute_batch(MIGRATION_1),
        1 => transaction
            .execute_batch(MIGRATION_2)
            .and_then(|()| transaction.execute_batch(MIGRATION_3))
            .and_then(|()| transaction.execute_batch(MIGRATION_4))
            .and_then(|()| transaction.execute_batch(MIGRATION_5))
            .and_then(|()| transaction.execute_batch(MIGRATION_6))
            .and_then(|()| transaction.execute_batch(MIGRATION_7)),
        2 => transaction
            .execute_batch(MIGRATION_3)
            .and_then(|()| transaction.execute_batch(MIGRATION_4))
            .and_then(|()| transaction.execute_batch(MIGRATION_5))
            .and_then(|()| transaction.execute_batch(MIGRATION_6))
            .and_then(|()| transaction.execute_batch(MIGRATION_7)),
        3 => transaction
            .execute_batch(MIGRATION_4)
            .and_then(|()| transaction.execute_batch(MIGRATION_5))
            .and_then(|()| transaction.execute_batch(MIGRATION_6))
            .and_then(|()| transaction.execute_batch(MIGRATION_7)),
        4 => transaction
            .execute_batch(MIGRATION_5)
            .and_then(|()| transaction.execute_batch(MIGRATION_6))
            .and_then(|()| transaction.execute_batch(MIGRATION_7)),
        5 => transaction
            .execute_batch(MIGRATION_6)
            .and_then(|()| transaction.execute_batch(MIGRATION_7)),
        6 => transaction.execute_batch(MIGRATION_7),
        _ => Ok(()),
    }
    .map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE schema_meta
             SET schema_version = ?1, storage_protocol = ?2, migrated_at = ?3
             WHERE singleton = 1",
            params![
                i64::from(SUPPORTED_SCHEMA_VERSION),
                i64::from(STORAGE_PROTOCOL_VERSION),
                at.as_millis()
            ],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE migration_history SET applied_at = ?1",
            [at.as_millis()],
        )
        .map_err(map_sql_error)?;
    transaction.commit().map_err(map_sql_error)
}

pub(super) fn create_backup(
    connection: &Connection,
    config: &StoreConfig,
    found: u32,
) -> Result<(), StoreError> {
    create_private_dir(&config.backup_dir)
        .map_err(|error| StoreError::Backup(error.to_string()))?;
    let path = reserve_backup_path(config, found)?;
    match connection.backup(rusqlite::MAIN_DB, &path, None) {
        Ok(()) => set_private_file_permissions(&path)
            .map_err(|error| StoreError::Backup(error.to_string())),
        Err(error) => {
            let _ = fs::remove_file(&path);
            Err(StoreError::Backup(error.to_string()))
        }
    }
}

fn reserve_backup_path(config: &StoreConfig, found: u32) -> Result<PathBuf, StoreError> {
    for suffix in 0..10_000_u32 {
        let path = config.backup_dir.join(format!(
            "proqi-before-v{}-{}-{}-{suffix}.sqlite3",
            found,
            config.migration_time.as_millis(),
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        set_private_open_mode(&mut options);
        match options.open(&path) {
            Ok(file) => {
                file.sync_all()
                    .map_err(|error| StoreError::Backup(error.to_string()))?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if path
                    .symlink_metadata()
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    return Err(StoreError::Backup(format!(
                        "unsafe backup destination is a symbolic link: {}",
                        path.display()
                    )));
                }
            }
            Err(error) => return Err(StoreError::Backup(error.to_string())),
        }
    }
    Err(StoreError::Backup(
        "could not reserve a unique backup filename".to_owned(),
    ))
}

pub(super) fn quick_check(connection: &Connection) -> Result<(), StoreError> {
    let result: String = connection
        .pragma_query_value(None, "quick_check", |row| row.get(0))
        .map_err(map_sql_error)?;
    if result == "ok" {
        Ok(())
    } else {
        Err(StoreError::Integrity(result))
    }
}

pub(super) fn retry_delay(base: Duration, attempt: u32, seed: u64) -> Duration {
    let multiplier = u64::from(attempt) + 1;
    let base_millis = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let mixed = seed
        .wrapping_add(u64::from(attempt).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let jitter = if base_millis == u64::MAX {
        0
    } else {
        mixed % base_millis.saturating_add(1)
    };
    Duration::from_millis(
        base_millis
            .saturating_mul(multiplier)
            .saturating_add(jitter),
    )
}
