//! Compatibility stamping across Reflow schema 15 and Browser history schema 16.

use super::DatabaseFixture;
use proqi::{
    adapters::sqlite::SqliteStore,
    ports::store::{MigrationMode, STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION, StoreError},
};

#[test]
fn fresh_database_records_current_compatibility_and_complete_migration_history() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("fixture");
    let metadata = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
        )
        .expect("metadata");
    assert_eq!(
        metadata,
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let versions = connection
        .prepare("SELECT version FROM migration_history ORDER BY version")
        .expect("history query")
        .query_map([], |row| row.get::<_, u32>(0))
        .expect("history rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("history versions");
    assert_eq!(versions, (1..=SUPPORTED_SCHEMA_VERSION).collect::<Vec<_>>());
}

#[test]
fn reflow_migration_requires_authority_and_preserves_a_pre_migration_backup() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("fixture");
    connection
        .execute_batch(
            "DROP TABLE browser_history_receipts;
             DROP TABLE browser_operation_receipts;
             DROP TABLE browser_operations;
             DROP TABLE browser_history_state;
             DELETE FROM migration_history WHERE version >= 15;
             UPDATE schema_meta SET schema_version = 14, storage_protocol = 13;",
        )
        .expect("prior stamp");
    drop(connection);
    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 14,
            supported: SUPPORTED_SCHEMA_VERSION
        })
    ));
    assert!(!fixture.config.backup_dir.exists());
    fixture.open().quick_check().expect("migrated integrity");
    let metadata = |path| {
        rusqlite::Connection::open(path)
            .expect("read metadata")
            .query_row(
                "SELECT schema_version, storage_protocol FROM schema_meta",
                [],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
            )
            .expect("metadata")
    };
    assert_eq!(
        metadata(fixture.config.database_path.clone()),
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let backups: Vec<_> = std::fs::read_dir(&fixture.config.backup_dir)
        .expect("backups")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(metadata(backups[0].clone()), (14, 13));
}

#[test]
fn browser_history_migrates_exact_reflow_schema_and_protocol() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("fixture");
    connection
        .execute_batch(
            "DROP TABLE browser_history_receipts;
             DROP TABLE browser_operation_receipts;
             DROP TABLE browser_operations;
             DROP TABLE browser_history_state;
             DELETE FROM migration_history WHERE version = 16;
             UPDATE schema_meta SET schema_version = 15, storage_protocol = 14;",
        )
        .expect("schema 15 stamp");
    drop(connection);

    fixture.open().quick_check().expect("migrated integrity");
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("migrated");
    let metadata = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
        )
        .expect("metadata");
    assert_eq!(
        metadata,
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let cursor = connection
        .query_row(
            "SELECT cursor FROM browser_history_state WHERE singleton = 1",
            [],
            |row| row.get::<_, u32>(0),
        )
        .expect("Browser cursor");
    assert_eq!(cursor, 0);
    let versions = connection
        .prepare("SELECT version FROM migration_history ORDER BY version")
        .expect("history query")
        .query_map([], |row| row.get::<_, u32>(0))
        .expect("history rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("history versions");
    assert_eq!(versions, (1..=16).collect::<Vec<_>>());
}
