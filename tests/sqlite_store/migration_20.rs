use super::*;

#[test]
fn physical_v19_database_adds_transfer_journal_under_forward_migration() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_998_000_000);
    let state = session_state(&mut ids, &test_path("proqi-transfer-migration"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    drop(store);
    let connection = Connection::open(&fixture.config.database_path).expect("physical database");
    connection
        .execute_batch(
            "DROP TABLE transfer_source_claims;
         DROP TABLE transfer_attempts;
         DELETE FROM migration_history WHERE version >= 20;
         UPDATE schema_meta SET schema_version = 19, storage_protocol = 18;",
        )
        .expect("restore physical v19 shape");
    drop(connection);

    let mut refuse = fixture.config.clone();
    refuse.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refuse),
        Err(StoreError::MigrationRequired {
            found: 19,
            supported: SUPPORTED_SCHEMA_VERSION
        })
    ));
    let mut migrated = fixture.open();
    migrated.quick_check().expect("integrity");
    assert_eq!(
        migrated
            .load_session(session_id)
            .expect("preserved session")
            .board
            .session
            .id,
        session_id
    );
    drop(migrated);
    let connection = Connection::open(&fixture.config.database_path).expect("migrated database");
    let count: i64 = connection
        .query_row("SELECT count(*) FROM transfer_attempts", [], |row| {
            row.get(0)
        })
        .expect("journal table");
    assert_eq!(count, 0);
    let version: i64 = connection
        .query_row("SELECT storage_protocol FROM schema_meta", [], |row| {
            row.get(0)
        })
        .expect("protocol");
    assert_eq!(version, i64::from(STORAGE_PROTOCOL_VERSION));
}
