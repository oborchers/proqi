//! Read-only SQLite health projection for the diagnostics adapter.

use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension as _};

/// Content-free database facts from a disposable snapshot.
pub struct SqliteHealth {
    /// Stored schema version.
    pub schema: u32,
    /// Stored protocol version.
    pub protocol: u32,
    /// Journal mode reported by SQLite.
    pub journal: String,
    /// Numeric SQLite synchronous mode.
    pub synchronous: i64,
    /// Complete quick-check result.
    pub integrity: String,
}

/// Inspect a disposable database snapshot without migration or writes.
///
/// # Errors
///
/// Returns a content-free SQLite validation failure.
pub fn inspect_read_only_snapshot(path: &Path) -> Result<SqliteHealth, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    let meta: Option<(i64, i64)> = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (schema, protocol) = meta.ok_or_else(|| "schema metadata is missing".to_owned())?;
    let schema = u32::try_from(schema).map_err(|_| "schema version is invalid".to_owned())?;
    let protocol = u32::try_from(protocol).map_err(|_| "storage protocol is invalid".to_owned())?;
    let journal = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let synchronous = connection
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let integrity = connection
        .pragma_query_value(None, "quick_check", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    Ok(SqliteHealth {
        schema,
        protocol,
        journal,
        synchronous,
        integrity,
    })
}
