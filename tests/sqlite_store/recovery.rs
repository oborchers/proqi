use super::*;

#[test]
fn unsupported_and_migration_required_schemas_are_refused_without_changes() {
    let fixture = DatabaseFixture::new();
    let mut refuse = fixture.config.clone();
    refuse.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refuse),
        Err(StoreError::MigrationRequired {
            found: 0,
            supported: 3
        })
    ));

    let newer = tempfile::tempdir().expect("newer fixture");
    let path = newer.path().join("newer.sqlite3");
    let connection = Connection::open(&path).expect("newer DB");
    connection
        .execute_batch(
            "CREATE TABLE schema_meta(singleton INTEGER, schema_version INTEGER);
             INSERT INTO schema_meta VALUES (1, 99);",
        )
        .expect("newer schema");
    drop(connection);
    let config = StoreConfig::new(
        path,
        newer.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    assert!(matches!(
        SqliteStore::open(&config),
        Err(StoreError::UnsupportedSchema {
            found: 99,
            supported: 3
        })
    ));

    let protocol = DatabaseFixture::new();
    drop(protocol.open());
    Connection::open(&protocol.config.database_path)
        .expect("protocol DB")
        .execute("UPDATE schema_meta SET storage_protocol = 99", [])
        .expect("newer protocol");
    assert!(matches!(
        SqliteStore::open(&protocol.config),
        Err(StoreError::UnsupportedStorageProtocol {
            found: 99,
            supported: 3
        })
    ));
}

#[test]
fn migration_backup_succeeds_and_failure_preserves_source() {
    let successful = tempfile::tempdir().expect("successful fixture");
    let successful_path = successful.path().join("legacy.sqlite3");
    let connection = Connection::open(&successful_path).expect("legacy");
    connection
        .execute_batch("CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES ('kept');")
        .expect("legacy data");
    drop(connection);
    let successful_config = StoreConfig::new(
        successful_path.clone(),
        successful.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(5),
    );
    let migrated = SqliteStore::open(&successful_config).expect("migrate");
    migrated.quick_check().expect("integrity");
    assert_eq!(
        Connection::open(&successful_path)
            .expect("verify legacy")
            .query_row("SELECT value FROM legacy", [], |row| row
                .get::<_, String>(0))
            .expect("legacy row"),
        "kept"
    );
    assert_eq!(
        std::fs::read_dir(&successful_config.backup_dir)
            .expect("backup directory")
            .count(),
        1
    );

    let failing = tempfile::tempdir().expect("failing fixture");
    let failing_path = failing.path().join("legacy.sqlite3");
    let connection = Connection::open(&failing_path).expect("legacy");
    connection
        .execute_batch("CREATE TABLE sessions(dummy TEXT); INSERT INTO sessions VALUES ('source');")
        .expect("conflicting table");
    drop(connection);
    let failing_config = StoreConfig::new(
        failing_path.clone(),
        failing.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(6),
    );
    assert!(SqliteStore::open(&failing_config).is_err());
    assert!(SqliteStore::open(&failing_config).is_err());
    let source = Connection::open(&failing_path).expect("preserved source");
    assert_eq!(
        source
            .query_row("SELECT dummy FROM sessions", [], |row| row
                .get::<_, String>(0))
            .expect("source row"),
        "source"
    );
    let meta_exists: bool = source
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'schema_meta')",
            [],
            |row| row.get(0),
        )
        .expect("meta check");
    assert!(!meta_exists);
    assert_eq!(
        std::fs::read_dir(&failing_config.backup_dir)
            .expect("failure backup")
            .count(),
        2
    );
}

#[test]
fn version_one_database_migrates_annotations_forward_without_reinterpretation() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = Connection::open(&fixture.config.database_path).expect("version one DB");
    connection
        .execute_batch(
            "DROP INDEX submission_attempts_active_thought;
             DROP TABLE submission_attempts;
             ALTER TABLE thoughts DROP COLUMN annotations_json;
             DELETE FROM migration_history WHERE version IN (2, 3);
             UPDATE schema_meta SET schema_version = 1, storage_protocol = 1;",
        )
        .expect("downgrade fixture");
    drop(connection);

    drop(SqliteStore::open(&fixture.config).expect("migrate version one"));
    let connection = Connection::open(&fixture.config.database_path).expect("migrated DB");
    let version: i64 = connection
        .query_row("SELECT schema_version FROM schema_meta", [], |row| {
            row.get(0)
        })
        .expect("version");
    let annotations_column: i64 = connection
        .query_row(
            "SELECT count(*) FROM pragma_table_info('thoughts') WHERE name = 'annotations_json'",
            [],
            |row| row.get(0),
        )
        .expect("annotation column");
    let submissions_table: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'submission_attempts'",
            [],
            |row| row.get(0),
        )
        .expect("submission table");
    assert_eq!((version, annotations_column, submissions_table), (3, 1, 1));
}

#[test]
fn malformed_database_is_reported_as_corrupt() {
    let temporary = tempfile::tempdir().expect("fixture");
    let database = temporary.path().join("corrupt.sqlite3");
    std::fs::write(&database, b"this is not a SQLite database").expect("corrupt fixture");
    let config = StoreConfig::new(
        database,
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    assert!(matches!(
        SqliteStore::open(&config),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn backup_failure_prevents_migration() {
    let temporary = tempfile::tempdir().expect("fixture");
    let database = temporary.path().join("legacy.sqlite3");
    Connection::open(&database)
        .expect("legacy")
        .execute("CREATE TABLE legacy(value TEXT)", [])
        .expect("legacy schema");
    let backup_file = temporary.path().join("not-a-directory");
    std::fs::write(&backup_file, b"occupied").expect("backup blocker");
    let config = StoreConfig::new(
        database.clone(),
        backup_file,
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    assert!(matches!(
        SqliteStore::open(&config),
        Err(StoreError::Backup(_))
    ));
    let connection = Connection::open(database).expect("source");
    let meta_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'schema_meta')",
            [],
            |row| row.get(0),
        )
        .expect("meta check");
    assert!(!meta_exists);
}

#[cfg(unix)]
#[test]
fn database_and_backup_permissions_are_user_only() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = DatabaseFixture::new();
    let _store = fixture.open();
    assert_eq!(
        std::fs::metadata(&fixture.config.database_path)
            .expect("database")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(
            fixture
                .config
                .database_path
                .parent()
                .expect("database parent")
        )
        .expect("data directory")
        .permissions()
        .mode()
            & 0o777,
        0o700
    );
}
