//! Schema 20 session request receipt protocol stamp and refusal contracts.

use std::path::PathBuf;

use proqi::{
    adapters::{memory::FakeIdGenerator, sqlite::SqliteStore},
    domain::{Session, Timestamp},
    ports::{
        environment::IdGenerator as _,
        store::{
            MigrationMode, NamedSessionCreation, NamedSessionOutcome, NamedSessionPolicy,
            STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION, SessionRequest, Store as _,
            StoreError, StoredSessionRequest,
        },
    },
};
use rusqlite::Connection;

use super::DatabaseFixture;

const DOWNGRADE_TO_19: &str = r"
DELETE FROM migration_history WHERE version = 20;
UPDATE schema_meta SET schema_version = 19, storage_protocol = 18;
";

#[test]
fn session_request_protocol_stamp_is_authorized_backed_up_and_preserves_receipts() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_100_000_000);
    let operation_id = ids.operation_id();
    let session_id = proqi::domain::SessionId::from_database_bytes(operation_id.database_bytes())
        .expect("derived session");
    let session = Session::with_name(
        session_id,
        PathBuf::from("/work"),
        Timestamp::from_millis(3),
        Some("stamped".to_owned()),
    )
    .expect("session");
    assert_eq!(
        store.create_named_session(&NamedSessionCreation {
            session,
            policy: NamedSessionPolicy::Always,
            operation_id: Some(operation_id),
        }),
        Ok(NamedSessionOutcome::Created)
    );
    drop(store);
    let connection = Connection::open(&fixture.config.database_path).expect("database");
    connection
        .execute_batch(DOWNGRADE_TO_19)
        .expect("schema 19 fixture");
    drop(connection);

    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 19,
            supported: SUPPORTED_SCHEMA_VERSION,
        })
    ));
    assert!(!fixture.config.backup_dir.exists());

    let mut store = fixture.open();
    store.quick_check().expect("migrated integrity");
    assert!(matches!(
        store.session_request(operation_id),
        Ok(Some(StoredSessionRequest::Administration(receipt)))
            if receipt.request
                == SessionRequest::create(session_id, "stamped", std::path::Path::new("/work"))
    ));
    drop(store);
    let connection = Connection::open(&fixture.config.database_path).expect("migrated");
    let metadata: (u32, u32) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("metadata");
    assert_eq!(metadata, (20, 19));
    assert_eq!(
        metadata,
        (SUPPORTED_SCHEMA_VERSION, STORAGE_PROTOCOL_VERSION)
    );
    let backups = std::fs::read_dir(&fixture.config.backup_dir)
        .expect("backup directory")
        .count();
    assert_eq!(backups, 1);
}

#[test]
fn a_newer_session_request_protocol_is_refused_before_writing() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = Connection::open(&fixture.config.database_path).expect("database");
    connection
        .execute(
            "UPDATE schema_meta SET storage_protocol = ?1",
            [i64::from(STORAGE_PROTOCOL_VERSION) + 1],
        )
        .expect("future protocol");
    drop(connection);
    assert!(matches!(
        SqliteStore::open(&fixture.config),
        Err(StoreError::UnsupportedStorageProtocol { .. })
    ));
}

#[test]
fn a_legacy_schema_19_rename_receipt_decodes_and_replays_after_migration() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_100_100_000);
    let session_id = ids.session_id();
    let session = Session::with_name(
        session_id,
        PathBuf::from("/work"),
        Timestamp::from_millis(3),
        Some("kept".to_owned()),
    )
    .expect("session");
    store
        .commit(&proqi::ports::store::OperationBatch::CreateSession(session))
        .expect("create");
    drop(store);
    let operation_id = ids.operation_id();
    // Exact payload bytes written by the published schema 19 binary.
    let legacy = format!(
        r#"{{"receipt":"rename_noop_v1","operation_id":"{operation_id}","session_id":"{session_id}","name":"kept"}}"#
    );
    let connection = Connection::open(&fixture.config.database_path).expect("database");
    connection
        .execute_batch(DOWNGRADE_TO_19)
        .expect("schema 19 fixture");
    connection
        .execute(
            "INSERT INTO browser_operation_receipts(
                 id, target_session_id, payload_json, cursor, created_at
             ) VALUES (?1, ?2, ?3, 0, 4)",
            rusqlite::params![
                operation_id.database_bytes().as_slice(),
                session_id.database_bytes().as_slice(),
                legacy,
            ],
        )
        .expect("legacy receipt");
    drop(connection);

    let mut store = fixture.open();
    assert!(matches!(
        store.session_request(operation_id),
        Ok(Some(StoredSessionRequest::Administration(receipt)))
            if receipt.request == SessionRequest::Rename {
                session_id,
                name: Some("kept".to_owned()),
            }
    ));
    let replay = store
        .commit_browser_noop_rename(
            operation_id,
            session_id,
            Some("kept"),
            Timestamp::from_millis(9),
        )
        .expect("replay");
    assert!(replay.idempotent_replay);
    assert!(matches!(
        store.commit_browser_noop_rename(
            operation_id,
            session_id,
            Some("other"),
            Timestamp::from_millis(9)
        ),
        Err(StoreError::Conflict(_))
    ));
}
