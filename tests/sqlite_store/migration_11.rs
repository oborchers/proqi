use super::*;

#[test]
fn protocol_ten_requires_lease_authorized_backup_before_migrating_to_eleven() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    Connection::open(&fixture.config.database_path)
        .expect("protocol ten fixture")
        .execute_batch(
            "DELETE FROM migration_history WHERE version = 11;
             UPDATE schema_meta SET schema_version = 10, storage_protocol = 10;",
        )
        .expect("downgrade protocol stamp");

    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 10,
            supported: 11
        })
    ));
    let connection = Connection::open(&fixture.config.database_path).expect("unchanged fixture");
    assert_eq!(
        connection
            .query_row("SELECT schema_version FROM schema_meta", [], |row| row
                .get::<_, u32>(0))
            .expect("schema version"),
        10
    );
    assert!(!fixture.config.backup_dir.exists());
    drop(connection);

    let migrated = fixture.open();
    migrated.quick_check().expect("migrated integrity");
    drop(migrated);
    let connection = Connection::open(&fixture.config.database_path).expect("migrated fixture");
    assert_eq!(
        connection
            .query_row(
                "SELECT schema_version, storage_protocol FROM schema_meta",
                [],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)),
            )
            .expect("current versions"),
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM migration_history WHERE version = 11",
                [],
                |row| row.get::<_, u32>(0),
            )
            .expect("migration history"),
        1
    );
    assert_eq!(
        std::fs::read_dir(&fixture.config.backup_dir)
            .expect("backup directory")
            .count(),
        1
    );
}
