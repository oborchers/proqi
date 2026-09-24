//! Schema 19 semantic receipt fingerprint migration and rollback contracts.

use proqi::{
    adapters::{memory::FakeIdGenerator, sqlite::SqliteStore},
    application::Action,
    domain::Timestamp,
    ports::{
        environment::IdGenerator as _,
        store::{
            MigrationMode, OperationBatch, STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION,
            Store as _, StoreError,
        },
    },
};
use rusqlite::Connection;

use super::{DatabaseFixture, one_effect, persist_effect, session_state, test_path};

const DOWNGRADE_TO_18: &str = r"
DROP TABLE IF EXISTS transfer_source_claims;
DROP TABLE IF EXISTS transfer_attempts;
ALTER TABLE commit_receipts DROP COLUMN semantic_fingerprint;
DELETE FROM migration_history WHERE version >= 19;
UPDATE schema_meta SET schema_version = 18, storage_protocol = 17;
";

#[test]
fn semantic_receipt_migration_is_additive_authorized_and_backed_up() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-migration-19"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let operation_id = ids.operation_id();
    let operation = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id,
            content: "legacy receipt Grüße".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    persist_effect(&mut store, &operation);
    drop(store);

    let connection = Connection::open(&fixture.config.database_path).expect("current database");
    let receipt_before: String = connection
        .query_row(
            "SELECT request_json FROM commit_receipts WHERE external_id = ?1",
            [operation_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("legacy receipt");
    connection
        .execute_batch(DOWNGRADE_TO_18)
        .expect("schema 18 fixture");
    drop(connection);

    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 18,
            supported: SUPPORTED_SCHEMA_VERSION,
        })
    ));
    assert!(!fixture.config.backup_dir.exists());

    fixture.open().quick_check().expect("migrated integrity");
    let connection = Connection::open(&fixture.config.database_path).expect("migrated database");
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
    let receipt_after: (String, Option<Vec<u8>>) = connection
        .query_row(
            "SELECT request_json, semantic_fingerprint FROM commit_receipts
             WHERE external_id = ?1",
            [operation_id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("migrated receipt");
    assert_eq!(receipt_after, (receipt_before, None));
    let backups = std::fs::read_dir(&fixture.config.backup_dir)
        .expect("backup directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("backups");
    assert_eq!(backups.len(), 1);
    assert!(store_session_exists(&connection, session_id));
}

#[test]
fn conflicting_semantic_receipt_column_rolls_back_and_exact_retry_recovers() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = Connection::open(&fixture.config.database_path).expect("current database");
    connection
        .execute_batch(
            "DROP TABLE IF EXISTS transfer_source_claims; DROP TABLE IF EXISTS transfer_attempts;",
        )
        .expect("remove later transfer schema");
    connection
        .execute("DELETE FROM migration_history WHERE version >= 19", [])
        .expect("remove current migration marker");
    connection
        .execute(
            "UPDATE schema_meta SET schema_version = 18, storage_protocol = 17",
            [],
        )
        .expect("stamp schema 18");
    drop(connection);

    assert!(SqliteStore::open(&fixture.config).is_err());
    let connection = Connection::open(&fixture.config.database_path).expect("failed migration");
    let metadata: (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("rolled back metadata");
    assert_eq!(metadata, (18, 17));
    connection
        .execute_batch("ALTER TABLE commit_receipts DROP COLUMN semantic_fingerprint;")
        .expect("remove conflicting column");
    drop(connection);

    fixture.open().quick_check().expect("retry succeeds");
}

fn store_session_exists(connection: &Connection, session_id: proqi::domain::SessionId) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("session existence")
}
