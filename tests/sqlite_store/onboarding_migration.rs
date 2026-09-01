//! Onboarding marker migration and mixed-version refusal across supported schemas.

use proqi::{
    adapters::{memory::FakeIdGenerator, sqlite::SqliteStore},
    application::{FirstRunEnvironment, first_run_board},
    domain::{Session, Timestamp},
    ports::{
        environment::IdGenerator as _,
        store::{
            FirstRunOutcome, STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION, Store, StoreError,
        },
    },
};
use rusqlite::Connection;

use super::{DatabaseFixture, test_path};

fn downgrade_to(connection: &Connection, version: u32) {
    connection
        .execute_batch("DROP TABLE onboarding_state;")
        .expect("remove onboarding marker");
    if version <= 6 {
        connection
            .execute_batch("DROP TABLE screenshot_capture_receipts;")
            .expect("remove screenshot receipts");
    }
    if version <= 5 {
        connection
            .execute_batch(
                "UPDATE thoughts SET collapsed = 1 WHERE presentation = 'collapsed';
                 ALTER TABLE thoughts DROP COLUMN presentation;",
            )
            .expect("remove presentation preference");
    }
    if version <= 4 {
        connection
            .execute_batch(
                "DROP INDEX submission_attempt_items_active_thought;
                 DROP TABLE submission_attempt_items;
                 CREATE UNIQUE INDEX submission_attempts_active_thought
                 ON submission_attempts(thought_id)
                 WHERE state IN ('prepared', 'sending');",
            )
            .expect("restore single-source submission schema");
    }
    if version <= 2 {
        connection
            .execute_batch(
                "DROP INDEX submission_attempts_active_thought;
                 DROP TABLE submission_attempts;",
            )
            .expect("remove submission journal");
    }
    if version == 1 {
        connection
            .execute_batch("ALTER TABLE thoughts DROP COLUMN annotations_json;")
            .expect("remove annotations");
    }
    connection
        .execute(
            "DELETE FROM migration_history WHERE version > ?1",
            [i64::from(version)],
        )
        .expect("truncate migration history");
    connection
        .execute(
            "UPDATE schema_meta SET schema_version = ?1, storage_protocol = ?1",
            [i64::from(version)],
        )
        .expect("set legacy version");
}

#[test]
fn every_supported_prior_schema_migrates_with_current_onboarding_completed() {
    for version in 1..SUPPORTED_SCHEMA_VERSION {
        let fixture = DatabaseFixture::new();
        drop(fixture.open());
        let connection = Connection::open(&fixture.config.database_path).expect("legacy database");
        downgrade_to(&connection, version);
        drop(connection);

        let mut migrated = fixture.open();
        let connection = Connection::open(&fixture.config.database_path).expect("migrated marker");
        let completed: i64 = connection
            .query_row(
                "SELECT completed_version FROM onboarding_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .expect("completed onboarding version");
        assert_eq!(completed, 1, "schema {version} must not become eligible");
        drop(connection);

        let mut ids = FakeIdGenerator::new(1_725_000_000_000 + u64::from(version));
        let session = Session::new(
            ids.session_id(),
            test_path(&format!("proqi-onboarding-migrated-{version}")),
            Timestamp::from_millis(2),
        )
        .expect("candidate session");
        let candidate = first_run_board(session, &mut ids, FirstRunEnvironment::Standalone)
            .expect("candidate board");
        assert_eq!(
            migrated
                .create_first_run_session(&candidate)
                .expect("post-migration launch"),
            FirstRunOutcome::AlreadyCompleted
        );
        assert!(
            migrated
                .load_session(candidate.board().session.id)
                .expect("empty post-migration session")
                .board
                .live_thoughts()
                .is_empty()
        );
    }
}

#[test]
fn prior_schema_with_future_storage_protocol_fails_closed_without_migration() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let connection = Connection::open(&fixture.config.database_path).expect("legacy database");
    let prior_schema = SUPPORTED_SCHEMA_VERSION - 1;
    let future_protocol = STORAGE_PROTOCOL_VERSION + 1;
    downgrade_to(&connection, prior_schema);
    connection
        .execute(
            "UPDATE schema_meta SET storage_protocol = ?1",
            [i64::from(future_protocol)],
        )
        .expect("set future protocol");
    drop(connection);

    assert!(matches!(
        SqliteStore::open(&fixture.config),
        Err(StoreError::UnsupportedStorageProtocol {
            found,
            supported: STORAGE_PROTOCOL_VERSION,
        }) if found == future_protocol
    ));
    let connection = Connection::open(&fixture.config.database_path).expect("unchanged database");
    let versions: (i64, i64) = connection
        .query_row(
            "SELECT schema_version, storage_protocol FROM schema_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("unchanged versions");
    assert_eq!(
        versions,
        (i64::from(prior_schema), i64::from(future_protocol))
    );
    let marker_exists: bool = connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM sqlite_master
                 WHERE type = 'table' AND name = 'onboarding_state'
             )",
            [],
            |row| row.get(0),
        )
        .expect("marker existence");
    assert!(!marker_exists);
}
