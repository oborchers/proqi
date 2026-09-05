use super::*;

const DOWNGRADE_TO_12: &str = r"
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;
DROP INDEX submission_attempt_items_active_thought;
ALTER TABLE submission_attempt_items RENAME TO submission_attempt_items_v13;
ALTER TABLE submission_attempts RENAME TO submission_attempts_v13;
CREATE TABLE submission_attempts (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    source_digest BLOB NOT NULL CHECK (length(source_digest) = 32),
    source_sequence INTEGER NOT NULL CHECK (source_sequence >= 0),
    disposition TEXT NOT NULL CHECK (disposition IN ('keep', 'remove_after_success')),
    direction TEXT NOT NULL CHECK (direction IN ('up', 'right', 'down', 'left')),
    provider TEXT NOT NULL,
    protocol INTEGER NOT NULL CHECK (protocol >= 0),
    target_fingerprint BLOB NOT NULL CHECK (length(target_fingerprint) = 32),
    pre_state TEXT NOT NULL,
    post_state TEXT,
    error_code TEXT,
    deletion_operation_id BLOB CHECK (
        deletion_operation_id IS NULL OR length(deletion_operation_id) = 16
    ),
    state TEXT NOT NULL CHECK (
        state IN ('prepared', 'sending', 'accepted', 'failed', 'cancelled', 'outcome_unknown')
    ),
    prepared_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
INSERT INTO submission_attempts(
    id, session_id, thought_id, source_digest, source_sequence, disposition, direction,
    provider, protocol, target_fingerprint, pre_state, post_state, error_code,
    deletion_operation_id, state, prepared_at, updated_at
)
SELECT id, session_id, thought_id, source_digest, source_sequence, disposition, direction,
       provider, protocol, target_fingerprint, pre_state, post_state, error_code,
       deletion_operation_id, state, prepared_at, updated_at
FROM submission_attempts_v13;
CREATE TABLE submission_attempt_items (
    submission_id BLOB NOT NULL REFERENCES submission_attempts(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_digest BLOB NOT NULL CHECK (length(source_digest) = 32),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    PRIMARY KEY(submission_id, ordinal),
    UNIQUE(submission_id, thought_id)
) STRICT;
INSERT INTO submission_attempt_items(submission_id, thought_id, ordinal, source_digest, active)
SELECT submission_id, thought_id, ordinal, source_digest, active
FROM submission_attempt_items_v13;
DROP TABLE submission_attempt_items_v13;
DROP TABLE submission_attempts_v13;
CREATE UNIQUE INDEX submission_attempt_items_active_thought
ON submission_attempt_items(thought_id)
WHERE active = 1;
DELETE FROM migration_history WHERE version = 13;
UPDATE schema_meta SET schema_version = 12, storage_protocol = 11;
COMMIT;
PRAGMA foreign_keys = ON;
";

fn legacy_attempt(
    ids: &mut FakeIdGenerator,
    state: &AppState,
    sources: Vec<SubmissionSource>,
    direction: Direction,
    marker: u8,
) -> SubmissionAttempt {
    SubmissionAttempt {
        id: ids.submission_id(),
        session_id: state.board.session.id,
        sources,
        payload_digest: [marker; 32],
        source_sequence: state.board.session.last_durable_sequence,
        disposition: SubmissionDisposition::Keep,
        route: proqi::ports::store::SubmissionJournalRoute::adjacent(direction),
        provider: "herdr".to_owned(),
        protocol: 20,
        target_fingerprint: [marker.saturating_add(1); 32],
        pre_state: AgentState::Working,
        prepared_at: Timestamp::from_millis(40),
    }
}

#[test]
fn physical_v12_route_migration_preserves_legacy_bytes_and_recovers_conservatively() {
    let (fixture, session_id) = physical_v12_database();
    let mut refused = fixture.config.clone();
    refused.migration_mode = MigrationMode::Refuse;
    assert!(matches!(
        SqliteStore::open(&refused),
        Err(StoreError::MigrationRequired {
            found: 12,
            supported: 13
        })
    ));

    let migrated = fixture.open();
    migrated.quick_check().expect("migrated integrity");
    let connection = Connection::open(&fixture.config.database_path).expect("migrated database");
    assert_legacy_route_rows(&connection);
    drop(connection);
    assert_conservative_recovery_and_restart(&fixture, migrated, session_id);
}

fn physical_v12_database() -> (DatabaseFixture, proqi::domain::SessionId) {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(13_000);
    let mut state = session_state(&mut ids, &test_path("migration-13"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "second", 3);
    let prepared = legacy_attempt(
        &mut ids,
        &state,
        vec![SubmissionSource {
            thought_id: first,
            source_digest: [31; 32],
        }],
        Direction::Left,
        41,
    );
    let sending = legacy_attempt(
        &mut ids,
        &state,
        vec![
            SubmissionSource {
                thought_id: first,
                source_digest: [32; 32],
            },
            SubmissionSource {
                thought_id: second,
                source_digest: [33; 32],
            },
        ],
        Direction::Right,
        42,
    );
    store
        .prepare_submission(&prepared)
        .expect("prepared attempt");
    store
        .finish_submission(
            prepared.id,
            &SubmissionOutcome {
                state: SubmissionAttemptState::Cancelled,
                post_state: None,
                error_code: Some("fixture".to_owned()),
                deletion_operation_id: None,
                at: Timestamp::from_millis(41),
            },
        )
        .expect_err("prepared cannot finish before sending");
    store
        .prepare_submission(&sending)
        .expect_err("shared source stays locked");
    store
        .recover_submissions(session_id, Timestamp::from_millis(42))
        .expect("release first lock");
    store.prepare_submission(&sending).expect("sending attempt");
    store
        .mark_submission_sending(sending.id, Timestamp::from_millis(43))
        .expect("mark sending");
    drop(store);

    let connection = Connection::open(&fixture.config.database_path).expect("current database");
    connection
        .execute_batch(DOWNGRADE_TO_12)
        .expect("physical v12 database");
    drop(connection);
    (fixture, session_id)
}

fn assert_legacy_route_rows(connection: &Connection) {
    let route_rows = connection
        .prepare(
            "SELECT route_version, route_kind, direction, target_fingerprint, state
             FROM submission_attempts ORDER BY prepared_at, id",
        )
        .expect("route query")
        .query_map([], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .expect("route rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect route rows");
    assert_eq!(route_rows.len(), 2);
    assert!(
        route_rows
            .iter()
            .all(|row| row.0 == 0 && row.1 == "adjacent_pane")
    );
    assert!(
        route_rows
            .iter()
            .any(|row| row.2.as_deref() == Some("left") && row.3 == [42; 32])
    );
    assert!(
        route_rows
            .iter()
            .any(|row| row.2.as_deref() == Some("right") && row.3 == [43; 32])
    );
}

fn assert_conservative_recovery_and_restart(
    fixture: &DatabaseFixture,
    mut migrated: SqliteStore,
    session_id: proqi::domain::SessionId,
) {
    migrated
        .recover_submissions(session_id, Timestamp::from_millis(44))
        .expect("conservative recovery");
    let connection = Connection::open(&fixture.config.database_path).expect("recovered database");
    let states = connection
        .prepare("SELECT state FROM submission_attempts ORDER BY prepared_at, id")
        .expect("state query")
        .query_map([], |row| row.get::<_, String>(0))
        .expect("states")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect states");
    assert!(states.contains(&"cancelled".to_owned()));
    assert!(states.contains(&"outcome_unknown".to_owned()));
    assert_eq!(
        std::fs::read_dir(&fixture.config.backup_dir)
            .expect("migration backup")
            .count(),
        1
    );
    drop(connection);
    drop(migrated);
    let reopened = fixture.open();
    reopened.quick_check().expect("idempotent restart");
    assert_eq!(
        std::fs::read_dir(&fixture.config.backup_dir)
            .expect("same backup set")
            .count(),
        1
    );
}
