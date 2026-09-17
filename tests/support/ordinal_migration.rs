//! Synthetic durable attachment history and exact SQLite preservation oracles.

use std::path::{Path, PathBuf};

use proqi::{
    adapters::{
        memory::FakeIdGenerator,
        sqlite::{SqliteStore, StoreConfig},
    },
    application::{Action, AppState, reduce},
    domain::{
        BoardOperationKind, ContentAnnotation, ContentAnnotationKind, Session, SessionBoard,
        SessionId, TextPosition, Timestamp,
    },
    ports::{
        environment::IdGenerator,
        store::{MigrationMode, OperationBatch, Store},
    },
};
use rusqlite::{Connection, types::Value};

#[path = "legacy_ordinals.rs"]
mod legacy_ordinals;
use legacy_ordinals::strip_ordinals;

pub struct Fixture {
    pub root: PathBuf,
    pub config: StoreConfig,
    pub session: SessionId,
}

impl Fixture {
    pub fn seed(root: &Path, count: usize) -> Self {
        let config = StoreConfig::new(
            root.join("data/proqi.sqlite3"),
            root.join("data/backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(900),
        );
        let mut store = SqliteStore::open(&config).expect("new store");
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            PathBuf::from("/synthetic"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        store
            .commit(&OperationBatch::CreateSession(session.clone()))
            .expect("create session");
        let mut state = AppState::new(SessionBoard::new(session.clone(), vec![]).expect("board"));
        for index in 0..count {
            seed_thought(&mut store, &mut state, &mut ids, index);
        }
        store
            .rename_session(session.id, Some("synthetic migration Grüße"))
            .expect("name");
        store
            .record_session_open(
                session.id,
                Path::new("/synthetic/reopened"),
                Timestamp::from_millis(800),
            )
            .expect("opening metadata");
        store.quick_check().expect("seed integrity");
        Self {
            root: root.to_owned(),
            config,
            session: session.id,
        }
    }

    pub fn connection(&self) -> Connection {
        Connection::open(&self.config.database_path).expect("fixture connection")
    }

    pub fn downgrade(&self, schema: u32) {
        assert!((13..=15).contains(&schema));
        let connection = self.connection();
        connection
            .execute_batch(
                "DROP TABLE browser_history_receipts;
            DROP TABLE browser_operation_receipts;
            DROP TABLE browser_operations;
            DROP TABLE browser_history_state;
            ALTER TABLE thoughts DROP COLUMN name;",
            )
            .expect("remove schema 16 and 17 state");
        connection
            .execute("DELETE FROM migration_history WHERE version > ?1", [schema])
            .expect("historical migration rows");
        connection
            .execute("UPDATE migration_history SET applied_at = version * 7", [])
            .expect("distinct historical timestamps");
        connection
            .execute(
                "UPDATE schema_meta SET schema_version = ?1, storage_protocol = ?2,
            migrated_at = 42",
                [schema, schema - 1],
            )
            .expect("historical metadata");
        if schema == 13 {
            strip_all_ordinals(&connection);
            connection
                .execute_batch(
                    "ALTER TABLE sessions DROP COLUMN attachment_image_high;
                ALTER TABLE sessions DROP COLUMN attachment_file_high;",
                )
                .expect("pre-ordinal columns");
        }
    }
}

fn seed_thought(
    store: &mut SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    index: usize,
) {
    let thought_id = ids.thought_id();
    let path = format!("/synthetic/Grüße 界 {index}.png");
    let content = format!("{path}\n\t e\u{301} 界\u{1b}[31m\u{7}\r\nexact text");
    let annotation = ContentAnnotation {
        start: 0,
        end: path.len(),
        kind: ContentAnnotationKind::Attachment {
            image: index.is_multiple_of(2),
            display_name: "Grüße 界".to_owned(),
            ordinal: None,
        },
    };
    persist(
        store,
        state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![annotation],
            insertion_index: None,
            at: Timestamp::from_millis(10),
        },
    );
    let annotations = state
        .board
        .thought(thought_id)
        .expect("thought")
        .annotations
        .clone();
    persist(
        store,
        state,
        Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: content.clone(),
            after_content: format!("{content}\nrevision"),
            before_annotations: annotations.clone(),
            after_annotations: annotations,
            before_cursor: TextPosition::new(1, 2),
            after_cursor: TextPosition::new(3, 1),
            at: Timestamp::from_millis(20),
        },
    );
    if index.is_multiple_of(3) {
        persist(
            store,
            state,
            Action::DeleteThought {
                thought_id,
                operation_id: ids.operation_id(),
                kind: BoardOperationKind::Delete,
                at: Timestamp::from_millis(30),
            },
        );
    }
}

fn persist(store: &mut SqliteStore, state: &mut AppState, action: Action) {
    for effect in reduce(state, action).expect("fixture action") {
        if let Some(batch) = effect.persistence_batch() {
            store.commit(&batch).expect("fixture commit");
        }
    }
}

pub const PAYLOADS: [(&str, &str, &str); 4] = [
    ("thoughts", "annotations_json", "id"),
    ("thought_revisions", "payload_json", "id"),
    ("board_operations", "payload_json", "id"),
    ("commit_receipts", "request_json", "external_id"),
];

fn strip_all_ordinals(connection: &Connection) {
    for (table, column, key) in PAYLOADS {
        let rows = query(connection, &format!("SELECT {key}, {column} FROM {table}"));
        for row in rows {
            let Value::Text(encoded) = &row[1] else {
                panic!("JSON text")
            };
            let mut value: serde_json::Value = serde_json::from_str(encoded).expect("payload");
            strip_ordinals(&mut value);
            connection
                .execute(
                    &format!("UPDATE {table} SET {column} = ?2 WHERE {key} = ?1"),
                    rusqlite::params![row[0], value.to_string()],
                )
                .expect("legacy payload");
        }
    }
}

pub fn query(connection: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut statement = connection.prepare(sql).expect("oracle query");
    let columns = statement.column_count();
    statement
        .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
        .expect("oracle rows")
        .collect::<Result<_, _>>()
        .expect("oracle values")
}

pub fn durable_rows(connection: &Connection) -> Vec<Vec<Vec<Value>>> {
    let has_name = query(connection, "PRAGMA table_info(thoughts)")
        .iter()
        .any(|column| column.get(1) == Some(&Value::Text("name".to_owned())));
    let thoughts = if has_name {
        query(
            connection,
            "SELECT id, session_id, content, name, annotations_json, position,
            created_at, updated_at, collapsed, presentation, deleted_at, editor_history_cursor
            FROM thoughts ORDER BY rowid",
        )
    } else {
        query(
            connection,
            "SELECT id, session_id, content, NULL AS name, annotations_json, position,
            created_at, updated_at, collapsed, presentation, deleted_at, editor_history_cursor
            FROM thoughts ORDER BY rowid",
        )
    };
    vec![
        query(connection, "SELECT * FROM sessions ORDER BY rowid"),
        thoughts,
        query(connection, "SELECT * FROM thought_revisions ORDER BY rowid"),
        query(connection, "SELECT * FROM board_operations ORDER BY rowid"),
        query(connection, "SELECT * FROM commit_receipts ORDER BY rowid"),
        query(
            connection,
            "SELECT * FROM integration_context ORDER BY rowid",
        ),
        query(connection, "SELECT * FROM onboarding_state ORDER BY rowid"),
        query(
            connection,
            "SELECT * FROM submission_attempts ORDER BY rowid",
        ),
        query(
            connection,
            "SELECT * FROM submission_attempt_items ORDER BY rowid",
        ),
    ]
}

pub fn without_ordinals(connection: &Connection) -> Vec<Vec<Vec<Value>>> {
    let mut rows = durable_rows(connection);
    rows[0] = query(
        connection,
        "SELECT id, name, origin_cwd, last_opened_cwd, created_at,
        last_opened_at, last_active_at, last_durable_sequence, board_history_cursor, deleted_at
        FROM sessions ORDER BY rowid",
    );
    for (table, column) in [(1, 4), (2, 5), (3, 4), (4, 4)] {
        for row in &mut rows[table] {
            let Value::Text(encoded) = &row[column] else {
                panic!("JSON payload at table {table}, column {column}: {row:?}")
            };
            let mut value = serde_json::from_str(encoded).expect("payload JSON");
            strip_ordinals(&mut value);
            row[column] = Value::Text(value.to_string());
        }
    }
    rows
}
