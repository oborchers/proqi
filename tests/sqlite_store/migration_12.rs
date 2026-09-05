use super::*;

fn schema_metadata(connection: &Connection) -> (u32, u32) {
    connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("schema metadata")
}

fn onboarding_completed_version(connection: &Connection) -> u32 {
    connection
        .query_row(
            "SELECT completed_version FROM onboarding_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("onboarding marker")
}

#[test]
fn schema_eleven_requires_lease_authorized_backup_before_transformation_protocol_migration() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    Connection::open(&fixture.config.database_path)
        .expect("schema eleven fixture")
        .execute_batch(
            "DELETE FROM migration_history WHERE version IN (12, 13);
             UPDATE schema_meta SET schema_version = 11, storage_protocol = 10;",
        )
        .expect("downgrade transformation protocol stamp");

    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 11,
            supported: 13
        })
    ));
    let connection = Connection::open(&fixture.config.database_path).expect("unchanged fixture");
    assert_eq!(schema_metadata(&connection), (11, 10));
    assert_eq!(onboarding_completed_version(&connection), 0);
    assert!(!fixture.config.backup_dir.exists());
    drop(connection);

    let migrated = fixture.open();
    migrated.quick_check().expect("migrated integrity");
    drop(migrated);
    let connection = Connection::open(&fixture.config.database_path).expect("migrated fixture");
    assert_eq!(
        schema_metadata(&connection),
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM migration_history WHERE version IN (12, 13)",
                [],
                |row| row.get::<_, u32>(0),
            )
            .expect("migration history"),
        2
    );
    assert_eq!(onboarding_completed_version(&connection), 0);
    assert_eq!(
        std::fs::read_dir(&fixture.config.backup_dir)
            .expect("backup directory")
            .count(),
        1
    );
}
