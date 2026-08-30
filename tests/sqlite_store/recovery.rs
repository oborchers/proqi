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
            supported: SUPPORTED_SCHEMA_VERSION
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
            supported: SUPPORTED_SCHEMA_VERSION
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
            supported: STORAGE_PROTOCOL_VERSION
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
            "DROP TABLE onboarding_state;
             DROP TABLE screenshot_capture_receipts;
             DROP INDEX submission_attempt_items_active_thought;
             DROP TABLE submission_attempt_items;
             DROP TABLE submission_attempts;
             ALTER TABLE thoughts DROP COLUMN presentation;
             ALTER TABLE thoughts DROP COLUMN annotations_json;
             DELETE FROM migration_history WHERE version IN (2, 3, 4, 5, 6, 7, 8, 9);
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
    assert_eq!(
        (version, annotations_column, submissions_table),
        (i64::from(SUPPORTED_SCHEMA_VERSION), 1, 1)
    );

    let fresh = DatabaseFixture::new();
    drop(fresh.open());
    let fresh_connection =
        Connection::open(&fresh.config.database_path).expect("fresh schema comparison");
    assert_eq!(
        schema_indexes(&connection),
        schema_indexes(&fresh_connection),
        "fresh and migrated databases must expose the same indexes"
    );
}

fn schema_indexes(connection: &Connection) -> Vec<(String, String)> {
    let mut statement = connection
        .prepare(
            "SELECT name, sql FROM sqlite_master
             WHERE type = 'index' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .expect("schema index query");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("schema index rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("schema index values")
}

#[test]
fn legacy_thought_collapsed_field_deserializes_without_losing_state() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let state = session_state(&mut ids, &test_path("proqi-legacy-thought"));
    let thought = proqi::domain::Thought::new(
        ids.thought_id(),
        state.board.session.id,
        "legacy".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let json = serde_json::to_string(&thought)
        .expect("serialize thought")
        .replace("\"presentation\":\"automatic\"", "\"collapsed\":true");
    let legacy: proqi::domain::Thought = serde_json::from_str(&json).expect("legacy thought");
    assert_eq!(legacy.presentation, ThoughtPresentation::Collapsed);
}

#[test]
fn version_five_collapsed_flag_migrates_to_the_canonical_presentation() {
    let (fixture, mut ids, session_id, thought_id) = version_five_fixture();

    let mut migrated = fixture.open();
    let snapshot = migrated.load_session(session_id).expect("migrated session");
    let thought = snapshot
        .board
        .thought(thought_id)
        .expect("migrated thought")
        .clone();
    assert_eq!(thought.presentation, ThoughtPresentation::Collapsed);
    let mut restored = AppState::from_snapshot(snapshot).expect("restore migrated history");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut migrated, &undo);
    assert_eq!(
        migrated
            .load_session(session_id)
            .expect("undo result")
            .board
            .thought(thought_id)
            .expect("thought after undo")
            .presentation,
        ThoughtPresentation::Automatic
    );
    let redo = one_effect(
        &mut restored,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut migrated, &redo);
    assert_eq!(
        migrated
            .load_session(session_id)
            .expect("redo result")
            .board
            .thought(thought_id)
            .expect("thought after redo")
            .presentation,
        ThoughtPresentation::Collapsed
    );
}

fn version_five_fixture() -> (
    DatabaseFixture,
    FakeIdGenerator,
    proqi::domain::SessionId,
    proqi::domain::ThoughtId,
) {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-version-five"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "legacy", 2);
    let collapse = one_effect(
        &mut state,
        Action::SetPresentation {
            operation_id: ids.operation_id(),
            thought_id,
            presentation: ThoughtPresentation::Collapsed,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &collapse);
    drop(store);
    downgrade_collapsed_fixture(&fixture);
    (fixture, ids, session_id, thought_id)
}

fn downgrade_collapsed_fixture(fixture: &DatabaseFixture) {
    let connection = Connection::open(&fixture.config.database_path).expect("version five DB");
    connection
        .execute(
            "UPDATE board_operations
             SET payload_json = replace(
                 replace(payload_json,
                     '\"mutation\":\"set_presentation\",\"presentation\":\"collapsed\"',
                     '\"mutation\":\"set_collapsed\",\"collapsed\":true'),
                 '\"mutation\":\"set_presentation\",\"presentation\":\"automatic\"',
                 '\"mutation\":\"set_collapsed\",\"collapsed\":false')",
            [],
        )
        .expect("literal legacy operation payloads");
    connection
        .execute_batch(
            "DROP TABLE onboarding_state;
             DROP TABLE screenshot_capture_receipts;
             UPDATE thoughts SET collapsed = 1 WHERE id IS NOT NULL;
             ALTER TABLE thoughts DROP COLUMN presentation;
             DELETE FROM migration_history WHERE version IN (6, 7, 8, 9);
             UPDATE schema_meta SET schema_version = 5, storage_protocol = 5;",
        )
        .expect("downgrade fixture");
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
fn sqlite_symlink_shapes_are_rejected_without_touching_targets() {
    use std::os::unix::fs::symlink;

    for suffix in ["", "-wal", "-shm", "-journal"] {
        let temporary = tempfile::tempdir().expect("fixture");
        let database = temporary.path().join("proqi.sqlite3");
        let target = temporary.path().join("target");
        std::fs::write(&target, b"untouched").expect("target");
        let mut linked = database.as_os_str().to_os_string();
        linked.push(suffix);
        symlink(&target, std::path::PathBuf::from(linked)).expect("symlink");
        let config = StoreConfig::new(
            database,
            temporary.path().join("backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(1),
        );
        assert!(matches!(SqliteStore::open(&config), Err(StoreError::Io(_))));
        assert_eq!(std::fs::read(&target).expect("target bytes"), b"untouched");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_backup_destination_fails_closed() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("fixture");
    let database = temporary.path().join("legacy.sqlite3");
    Connection::open(&database)
        .expect("legacy database")
        .execute("CREATE TABLE legacy(value TEXT)", [])
        .expect("legacy table");
    let backups = temporary.path().join("backups");
    std::fs::create_dir(&backups).expect("backup directory");
    let target = temporary.path().join("target");
    std::fs::write(&target, b"untouched").expect("target");
    let timestamp = Timestamp::from_millis(7);
    let destination = backups.join(format!(
        "proqi-before-v0-{}-{}-0.sqlite3",
        timestamp.as_millis(),
        std::process::id()
    ));
    symlink(&target, destination).expect("backup destination symlink");
    let config = StoreConfig::new(database, backups, MigrationMode::Allow, timestamp);
    assert!(matches!(
        SqliteStore::open(&config),
        Err(StoreError::Backup(message)) if message.contains("symbolic link")
    ));
    assert_eq!(std::fs::read(&target).expect("target bytes"), b"untouched");
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
