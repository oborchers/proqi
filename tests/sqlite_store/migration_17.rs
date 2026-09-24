use super::*;
use proqi::domain::{OperationId, SessionId};

const DOWNGRADE_TO_16: &str = r"
ALTER TABLE commit_receipts DROP COLUMN semantic_fingerprint;
ALTER TABLE thoughts DROP COLUMN name;
DROP INDEX separators_session;
DROP INDEX separators_live_position;
DROP TABLE separators;
DROP TABLE IF EXISTS transfer_source_claims;
             DROP TABLE IF EXISTS transfer_attempts;
             DELETE FROM migration_history WHERE version >= 17;
UPDATE schema_meta SET schema_version = 16, storage_protocol = 15;
";

struct MigrationSeed {
    session_id: SessionId,
    thought_id: ThoughtId,
    content: String,
    annotation: ContentAnnotation,
    receipt_operation_id: OperationId,
    browser_operation_id: OperationId,
}

fn seed_migration_fixture(fixture: &DatabaseFixture) -> MigrationSeed {
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_997_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-migration-17"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = ids.thought_id();
    let content = "/tmp/Grüße.png ".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len() - 1,
        kind: ContentAnnotationKind::Attachment {
            ordinal: Some(1_u64.try_into().expect("ordinal")),
            image: true,
            display_name: "Grüße.png".to_owned(),
        },
    };
    let effects = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![annotation.clone()],
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("create thought");
    let create = effects
        .iter()
        .find(|effect| matches!(effect, Effect::CommitBoardOperation(_)))
        .expect("durable thought effect");
    let receipt = persist_effect(&mut store, create);
    let browser_operation = BrowserOperation::rename(
        ids.operation_id(),
        session_id,
        None,
        Some("Migrated".to_owned()),
        Timestamp::from_millis(3),
    )
    .expect("rename operation");
    store
        .commit_browser_operation(&browser_operation)
        .expect("browser operation");
    let DurableIdentity::Operation(receipt_operation_id) = receipt.identity else {
        panic!("operation receipt");
    };
    MigrationSeed {
        session_id,
        thought_id,
        content,
        annotation,
        receipt_operation_id,
        browser_operation_id: browser_operation.id(),
    }
}

fn durable_bytes(
    connection: &Connection,
    seed: &MigrationSeed,
) -> (String, String, String, String) {
    connection
        .query_row(
            "SELECT
                (SELECT annotations_json FROM thoughts WHERE id = ?1),
                (SELECT payload_json FROM board_operations WHERE id = ?2),
                (SELECT request_json FROM commit_receipts WHERE external_id = ?2),
                (SELECT payload_json FROM browser_operations WHERE id = ?3)",
            rusqlite::params![
                seed.thought_id.database_bytes().as_slice(),
                seed.receipt_operation_id.database_bytes().as_slice(),
                seed.browser_operation_id.database_bytes().as_slice(),
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("durable bytes")
}

#[test]
fn schema_16_upgrade_preserves_ordinal_content_history_receipts_and_browser_state() {
    let fixture = DatabaseFixture::new();
    let seed = seed_migration_fixture(&fixture);

    let connection = Connection::open(&fixture.config.database_path).expect("current database");
    connection
        .execute_batch(DOWNGRADE_TO_16)
        .expect("schema 16 fixture");
    let before = durable_bytes(&connection, &seed);
    drop(connection);

    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 16,
            supported: SUPPORTED_SCHEMA_VERSION
        })
    ));
    let mut migrated = fixture.open();
    migrated.quick_check().expect("migration integrity");
    let snapshot = migrated
        .load_session(seed.session_id)
        .expect("migrated snapshot");
    let thought = snapshot.board.thought(seed.thought_id).expect("thought");
    assert_eq!(thought.content, seed.content);
    assert_eq!(thought.annotations, std::slice::from_ref(&seed.annotation));
    assert!(snapshot.board.separators().is_empty());
    drop(migrated);

    let connection = Connection::open(&fixture.config.database_path).expect("migrated database");
    let after = durable_bytes(&connection, &seed);
    assert_eq!(after, before);
    assert_eq!(
        connection
            .query_row("SELECT cursor FROM browser_history_state", [], |row| row
                .get::<_, u32>(0))
            .expect("browser cursor"),
        1
    );
    let versions = connection
        .prepare("SELECT version FROM migration_history ORDER BY version")
        .expect("history")
        .query_map([], |row| row.get::<_, u32>(0))
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("versions");
    assert_eq!(versions, (1..=SUPPORTED_SCHEMA_VERSION).collect::<Vec<_>>());
    drop(connection);
    fixture.open().quick_check().expect("repeated open");
}

#[test]
fn failed_separator_migration_rolls_back_and_can_retry_after_conflict_is_removed() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = Connection::open(&fixture.config.database_path).expect("current database");
    connection
        .execute_batch(DOWNGRADE_TO_16)
        .expect("schema 16 fixture");
    connection
        .execute_batch("CREATE TABLE separators(marker TEXT) STRICT;")
        .expect("conflicting table");
    drop(connection);

    assert!(SqliteStore::open(&fixture.config).is_err());
    let connection = Connection::open(&fixture.config.database_path).expect("failed migration");
    assert_eq!(
        connection
            .query_row("SELECT schema_version FROM schema_meta", [], |row| row
                .get::<_, u32>(0))
            .expect("version"),
        16
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM migration_history WHERE version = 17",
                [],
                |row| row.get::<_, u32>(0),
            )
            .expect("history"),
        0
    );
    connection
        .execute_batch("DROP TABLE separators;")
        .expect("remove conflict");
    drop(connection);
    fixture.open().quick_check().expect("retry succeeds");
}
