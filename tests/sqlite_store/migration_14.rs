//! Compatibility stamping for the new Reflow durable operation kind.

use super::DatabaseFixture;
use proqi::{
    adapters::sqlite::SqliteStore,
    ports::store::{MigrationMode, StoreError},
};

#[test]
fn reflow_migration_requires_authority_and_preserves_a_pre_migration_backup() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("fixture");
    connection.execute_batch("DELETE FROM migration_history WHERE version = 14; UPDATE schema_meta SET schema_version = 13, storage_protocol = 12;").expect("prior stamp");
    drop(connection);
    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 13,
            supported: 14
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
    assert_eq!(metadata(fixture.config.database_path.clone()), (14, 13));
    let backups: Vec<_> = std::fs::read_dir(&fixture.config.backup_dir)
        .expect("backups")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(metadata(backups[0].clone()), (13, 12));
}
