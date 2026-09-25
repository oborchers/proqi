//! Schema 22 export operation-kind protocol stamp and restart-safe export history.

use std::path::PathBuf;

use proqi::{
    adapters::sqlite::SqliteStore,
    application::{ExportBoardChange, ExportCompletion},
    ports::store::{MigrationMode, STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION},
};

use super::{
    Action, BoardOperationKind, DatabaseFixture, FakeIdGenerator, IdGenerator as _, OperationBatch,
    Store as _, StoreError, Timestamp, UndoScope, create_thought, one_effect, persist_effect,
    reduce, session_state, test_path,
};

const DOWNGRADE_TO_21: &str = r"
DELETE FROM migration_history WHERE version >= 22;
UPDATE schema_meta SET schema_version = 21, storage_protocol = 20;
";

fn metadata(path: &std::path::Path) -> (u32, u32) {
    rusqlite::Connection::open(path)
        .expect("read metadata")
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("metadata")
}

#[test]
fn export_kind_stamp_requires_authority_backs_up_and_preserves_board_content() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_200_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-export-stamp"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    create_thought(&mut store, &mut state, &mut ids, "kept across the stamp", 2);
    drop(store);
    rusqlite::Connection::open(&fixture.config.database_path)
        .expect("database")
        .execute_batch(DOWNGRADE_TO_21)
        .expect("schema 21 fixture");

    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 21,
            supported: SUPPORTED_SCHEMA_VERSION,
        })
    ));
    assert!(!fixture.config.backup_dir.exists());

    let mut store = fixture.open();
    store.quick_check().expect("migrated integrity");
    let snapshot = store.load_session(session_id).expect("load");
    assert_eq!(
        snapshot.board.live_thoughts()[0].content,
        "kept across the stamp"
    );
    drop(store);
    assert_eq!(metadata(&fixture.config.database_path), (22, 21));
    assert_eq!(
        metadata(&fixture.config.database_path),
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let backups = std::fs::read_dir(&fixture.config.backup_dir)
        .expect("backups")
        .map(|entry| entry.expect("entry").path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(metadata(&backups[0]), (21, 20));
}

#[test]
fn a_protocol_newer_than_export_history_is_refused_before_writing() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    rusqlite::Connection::open(&fixture.config.database_path)
        .expect("database")
        .execute(
            "UPDATE schema_meta SET storage_protocol = ?1",
            [i64::from(STORAGE_PROTOCOL_VERSION) + 1],
        )
        .expect("future protocol");
    assert!(matches!(
        SqliteStore::open(&fixture.config),
        Err(StoreError::UnsupportedStorageProtocol { .. })
    ));
}

#[test]
fn export_replacement_is_one_restart_safe_undo_unit() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_300_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-export-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "second", 3);
    let reference = ids.thought_id();
    let expected_sources = [first, second]
        .iter()
        .map(|id| state.board.thought(*id).expect("source").clone())
        .collect();
    let effects = reduce(
        &mut state,
        Action::CompleteExport(ExportCompletion {
            operation_id: ids.operation_id(),
            thought_ids: vec![first, second],
            expected_sources,
            change: ExportBoardChange::ReplaceWithReference {
                reference_thought_id: reference,
                path: PathBuf::from("/exports/notes.txt"),
            },
            at: Timestamp::from_millis(4),
        }),
    )
    .expect("replace");
    persist_effect(&mut store, &effects[0]);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reload");
    assert_eq!(
        snapshot.board_operations.last().expect("operation").kind,
        BoardOperationKind::ExportAndReplace
    );
    let live = snapshot.board.live_thoughts();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, reference);
    assert_eq!(live[0].content, "/exports/notes.txt ");

    let mut state = proqi::application::AppState::from_snapshot(snapshot).expect("rehydrate");
    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut store, &undo);
    drop(store);
    let snapshot = fixture.open().load_session(session_id).expect("after undo");
    let contents = snapshot
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.clone())
        .collect::<Vec<_>>();
    assert_eq!(contents, ["first", "second"]);
}
