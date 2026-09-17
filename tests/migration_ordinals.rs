//! Real store and CLI migration boundaries preserve already-assigned attachment identities.

#[path = "migration_ordinals/cli_contract.rs"]
mod cli_contract;
#[path = "support/ordinal_migration.rs"]
mod ordinal_fixture;
use ordinal_fixture as support;

use proqi::{
    adapters::sqlite::SqliteStore,
    ports::store::{Store, StoreError},
};
use support::{Fixture, durable_rows, query};

#[test]
fn cli_migrates_ordinal_bearing_schema_boundaries_and_reopens() {
    let root = tempfile::tempdir().expect("isolated CLI state");
    cli_contract::assert_cli_migrations(
        || std::process::Command::new(env!("CARGO_BIN_EXE_proqi")),
        root.path(),
    );
}

#[test]
fn true_legacy_ordinals_in_each_durable_location_fail_atomically_and_retry() {
    for (table, column, _) in support::PAYLOADS {
        let root = tempfile::tempdir().expect("isolated legacy state");
        let fixture = Fixture::seed(root.path(), 6);
        fixture.downgrade(13);
        let path = match table {
            "thoughts" => "$[0].kind.ordinal",
            "thought_revisions" => "$.before_annotations[0].kind.ordinal",
            _ => "$.forward.thought.annotations[0].kind.ordinal",
        };
        let connection = fixture.connection();
        connection
            .execute(
                &format!(
                    "UPDATE {table} SET {column} = json_set({column}, ?1, 37)
            WHERE rowid = (SELECT min(rowid) FROM {table})"
                ),
                [path],
            )
            .expect("malformed legacy ordinal");
        let before = durable_rows(&connection);
        let metadata = query(&connection, "SELECT * FROM schema_meta");
        let migrations = query(
            &connection,
            "SELECT * FROM migration_history ORDER BY version",
        );
        let schema = query(
            &connection,
            "SELECT type, name, sql FROM sqlite_master ORDER BY name",
        );
        for _ in 0..2 {
            let Err(error) = SqliteStore::open(&fixture.config) else {
                panic!("legacy ordinal accepted")
            };
            assert_eq!(
                error,
                StoreError::Corrupt("ordinal in legacy schema".to_owned())
            );
            assert_eq!(
                error.to_string(),
                "storage is corrupt or malformed: ordinal in legacy schema"
            );
            assert_eq!(durable_rows(&connection), before);
            assert_eq!(query(&connection, "SELECT * FROM schema_meta"), metadata);
            assert_eq!(
                query(
                    &connection,
                    "SELECT * FROM migration_history ORDER BY version"
                ),
                migrations
            );
            assert_eq!(
                query(
                    &connection,
                    "SELECT type, name, sql FROM sqlite_master ORDER BY name"
                ),
                schema
            );
        }
        connection
            .execute(
                &format!(
                    "UPDATE {table} SET {column} = json_remove({column}, ?1)
            WHERE rowid = (SELECT min(rowid) FROM {table})"
                ),
                [path],
            )
            .expect("repair synthetic corruption");
        let mut store = SqliteStore::open(&fixture.config).expect("retry after fixture repair");
        store
            .load_session(fixture.session)
            .expect("resumable after repair");
        store.quick_check().expect("repaired integrity");
        drop(store);
        let synthesized = durable_rows(&fixture.connection());
        drop(SqliteStore::open(&fixture.config).expect("reopen synthesized state"));
        assert_eq!(durable_rows(&fixture.connection()), synthesized);
    }
}

#[test]
fn legacy_synthesis_numbers_live_first_and_dormant_history_once() {
    let root = tempfile::tempdir().expect("legacy state");
    let fixture = Fixture::seed(root.path(), 6);
    fixture.downgrade(13);
    let before = support::without_ordinals(&fixture.connection());
    let mut store = SqliteStore::open(&fixture.config).expect("legacy migration");
    let snapshot = store.load_session(fixture.session).expect("legacy session");
    assert_eq!(snapshot.board.attachment_counters().image(), 3);
    assert_eq!(snapshot.board.attachment_counters().file(), 3);
    assert_eq!(
        query(
            &fixture.connection(),
            "SELECT json_extract(annotations_json, '$[0].kind.ordinal') FROM thoughts ORDER BY id"
        ),
        [3, 1, 1, 3, 2, 2].map(|value| vec![value.into()]).to_vec()
    );
    assert_eq!(support::without_ordinals(&fixture.connection()), before);
}

#[test]
fn schema_15_preserves_assigned_ordinals_and_all_durable_history() {
    assert_preservation(15);
}

#[test]
fn schema_14_preserves_assigned_ordinals_and_all_durable_history() {
    assert_preservation(14);
}

#[test]
fn ordinal_schema_preserves_counters_after_all_attachment_history_is_pruned() {
    for schema in [14, 15] {
        let root = tempfile::tempdir().expect("pruned state");
        let fixture = Fixture::seed(root.path(), 0);
        fixture
            .connection()
            .execute(
                "UPDATE sessions SET attachment_image_high = 113,
            attachment_file_high = 127",
                [],
            )
            .expect("retained high water marks");
        fixture.downgrade(schema);
        let before = durable_rows(&fixture.connection());
        drop(SqliteStore::open(&fixture.config).expect("migrate pruned history"));
        assert_eq!(durable_rows(&fixture.connection()), before);
    }
}

fn assert_preservation(schema: u32) {
    let root = tempfile::tempdir().expect("isolated state");
    let fixture = Fixture::seed(root.path(), 128);
    fixture.downgrade(schema);
    let before = durable_rows(&fixture.connection());
    let migrations = query(
        &fixture.connection(),
        "SELECT * FROM migration_history ORDER BY version",
    );
    for _ in 0..3 {
        let mut store = SqliteStore::open(&fixture.config).unwrap_or_else(|error| {
            assert_eq!(
                error,
                StoreError::Corrupt("ordinal in legacy schema".to_owned())
            );
            panic!("valid schema {schema} rejected: {error}");
        });
        let snapshot = store
            .load_session(fixture.session)
            .expect("exact session resumable");
        assert_eq!(snapshot.board.attachment_counters().image(), 64);
        assert_eq!(snapshot.board.attachment_counters().file(), 64);
        store.quick_check().expect("migrated integrity");
        drop(store);
        assert_eq!(durable_rows(&fixture.connection()), before);
        assert_eq!(
            query(
                &fixture.connection(),
                &format!(
                    "SELECT * FROM migration_history WHERE version <= {schema} ORDER BY version"
                )
            ),
            migrations
        );
        assert_eq!(
            query(
                &fixture.connection(),
                "SELECT schema_version, storage_protocol FROM schema_meta"
            ),
            vec![vec![16.into(), 15.into()]]
        );
        assert_eq!(
            query(
                &fixture.connection(),
                "SELECT cursor FROM browser_history_state"
            ),
            vec![vec![0.into()]]
        );
    }
    let backups = std::fs::read_dir(&fixture.config.backup_dir)
        .expect("backup directory")
        .map(|entry| entry.expect("backup entry").path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).expect("pre-migration backup");
    assert_eq!(durable_rows(&backup), before);
    assert_eq!(
        query(&backup, "SELECT * FROM migration_history ORDER BY version"),
        migrations
    );
    assert_eq!(
        query(
            &backup,
            "SELECT schema_version, storage_protocol FROM schema_meta"
        ),
        vec![vec![i64::from(schema).into(), i64::from(schema - 1).into()]]
    );
    assert_eq!(
        query(&backup, "PRAGMA quick_check"),
        vec![vec!["ok".to_owned().into()]]
    );
}
