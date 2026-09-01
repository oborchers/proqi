use super::*;
use proqi::{
    application::{apply_capture, prepare_capture},
    ports::{
        screenshot::{ScreenshotCandidate, ScreenshotFingerprint, ScreenshotImageType},
        store::CaptureCommitOutcome,
    },
};

fn prepared_capture(
    state: &AppState,
    ids: &mut FakeIdGenerator,
    byte: u8,
    at: i64,
) -> proqi::ports::store::CaptureCommit {
    prepare_capture(
        state,
        &ScreenshotCandidate {
            fingerprint: ScreenshotFingerprint([byte; 32]),
            path: test_path(&format!("capture-{byte}.png")),
            image_type: ScreenshotImageType::Png,
        },
        ids.thought_id(),
        ids.operation_id(),
        Timestamp::from_millis(at),
    )
    .expect("prospective capture")
}

#[test]
fn capture_receipt_and_thought_commit_atomically_and_deduplicate_globally() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_240_000_000);
    let mut state = session_state(&mut ids, &test_path("screenshot-receipt"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let candidate = ScreenshotCandidate {
        fingerprint: ScreenshotFingerprint([7; 32]),
        path: test_path("Unicode capture 🖼️.png"),
        image_type: ScreenshotImageType::Png,
    };
    let commit = prepare_capture(
        &state,
        &candidate,
        ids.thought_id(),
        ids.operation_id(),
        Timestamp::from_millis(2),
    )
    .expect("prospective capture");

    let created = store.commit_capture(&commit).expect("atomic capture");
    assert!(matches!(created, CaptureCommitOutcome::Created { .. }));
    let thought_id = apply_capture(&mut state, &commit, &created)
        .expect("apply durable capture")
        .expect("created thought");
    let thought = state.board.thought(thought_id).expect("thought");
    let path = candidate.path.to_string_lossy();
    assert_eq!(thought.content, format!("{path} "));
    assert!(matches!(
        thought.annotations.as_slice(),
        [ContentAnnotation {
            start: 0,
            end,
            kind: ContentAnnotationKind::Attachment { image: true, .. },
        }] if *end == path.len()
    ));

    let duplicate = store.commit_capture(&commit).expect("deduplicate retry");
    assert!(matches!(
        duplicate,
        CaptureCommitOutcome::AlreadyCaptured(_)
    ));
    let snapshot = store
        .load_session(state.board.session.id)
        .expect("load captured session");
    assert_eq!(snapshot.board.thoughts().len(), 1);
    assert_eq!(
        snapshot.board.live_thoughts()[0].content,
        format!("{path} ")
    );
    let receipt_count: i64 = Connection::open(&fixture.config.database_path)
        .expect("inspect database")
        .query_row(
            "SELECT COUNT(*) FROM screenshot_capture_receipts",
            [],
            |row| row.get(0),
        )
        .expect("receipt count");
    assert_eq!(receipt_count, 1);
}

#[test]
fn invalid_capture_operation_leaves_neither_receipt_nor_partial_thought() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_241_000_000);
    let state = session_state(&mut ids, &test_path("screenshot-rollback"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let candidate = ScreenshotCandidate {
        fingerprint: ScreenshotFingerprint([9; 32]),
        path: test_path("capture.png"),
        image_type: ScreenshotImageType::Png,
    };
    let mut commit = prepare_capture(
        &state,
        &candidate,
        ids.thought_id(),
        ids.operation_id(),
        Timestamp::from_millis(2),
    )
    .expect("prospective capture");
    commit.operation.forward = BoardMutation::SetPresentation {
        thought_id: ids.thought_id(),
        presentation: ThoughtPresentation::Collapsed,
    };

    assert!(store.commit_capture(&commit).is_err());
    let snapshot = store
        .load_session(state.board.session.id)
        .expect("load unchanged session");
    assert!(snapshot.board.thoughts().is_empty());
    let receipt_count: i64 = Connection::open(&fixture.config.database_path)
        .expect("inspect database")
        .query_row(
            "SELECT COUNT(*) FROM screenshot_capture_receipts",
            [],
            |row| row.get(0),
        )
        .expect("receipt count");
    assert_eq!(receipt_count, 0);
}

#[test]
fn persistent_contention_leaves_no_partial_capture_and_exact_retry_succeeds() {
    let fixture = DatabaseFixture::new();
    let mut setup = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_241_500_000);
    let state = session_state(&mut ids, &test_path("screenshot-busy-retry"));
    setup
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    drop(setup);
    let mut config = fixture.config.clone();
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 2,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut store = SqliteStore::open(&config).expect("store");
    let capture = prepared_capture(&state, &mut ids, 13, 2);
    let raw = Connection::open(&config.database_path).expect("contending connection");
    raw.execute_batch("BEGIN IMMEDIATE").expect("writer lock");

    assert_eq!(store.commit_capture(&capture), Err(StoreError::Busy));
    assert!(
        store
            .load_session(state.board.session.id)
            .expect("unchanged session")
            .board
            .thoughts()
            .is_empty()
    );
    raw.execute_batch("ROLLBACK").expect("release writer");

    let outcome = store.commit_capture(&capture).expect("retry capture");
    assert!(matches!(outcome, CaptureCommitOutcome::Created { .. }));
    let snapshot = store
        .load_session(state.board.session.id)
        .expect("durable retry");
    let thought = snapshot.board.live_thoughts()[0];
    assert_eq!(
        thought.content,
        format!("{} ", test_path("capture-13.png").display())
    );
    assert_eq!(thought.annotations[0].end, thought.content.len() - 1);
    assert!(matches!(
        store.commit_capture(&capture).expect("dedupe replay"),
        CaptureCommitOutcome::AlreadyCaptured(_)
    ));
}

#[test]
fn capture_separator_survives_restart_undo_and_redo_without_duplication() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_241_750_000);
    let mut state = session_state(&mut ids, &test_path("screenshot-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let capture = prepared_capture(&state, &mut ids, 14, 2);
    let outcome = store.commit_capture(&capture).expect("capture");
    let thought_id = apply_capture(&mut state, &capture, &outcome)
        .expect("apply capture")
        .expect("created thought");
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("restart capture");
    let mut restored = AppState::from_snapshot(snapshot).expect("restore capture");
    assert_eq!(
        restored.board.thought(thought_id).expect("thought").content,
        format!("{} ", test_path("capture-14.png").display())
    );
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &undo);
    let redo_effects = reduce(
        &mut restored,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(4),
        },
    )
    .expect("redo capture");
    let redo = redo_effects
        .iter()
        .find(|effect| matches!(effect, Effect::CommitHistoryMove { .. }))
        .expect("durable redo");
    persist_effect(&mut store, redo);
    let redone = store.load_session(session_id).expect("redone capture");
    assert_eq!(redone.board.live_thoughts().len(), 1);
    assert_eq!(
        redone.board.live_thoughts()[0].content,
        format!("{} ", test_path("capture-14.png").display())
    );
}

#[test]
fn capture_receipt_survives_undo_then_branching_history() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_242_000_000);
    let mut state = session_state(&mut ids, &test_path("screenshot-branch"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let capture = prepared_capture(&state, &mut ids, 10, 2);
    let outcome = store.commit_capture(&capture).expect("capture");
    apply_capture(&mut state, &capture, &outcome).expect("apply capture");

    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &undo);
    create_thought(&mut store, &mut state, &mut ids, "new branch", 4);

    assert!(matches!(
        store.commit_capture(&capture).expect("global dedupe"),
        CaptureCommitOutcome::AlreadyCaptured(_)
    ));
    let snapshot = store
        .load_session(state.board.session.id)
        .expect("branched snapshot");
    assert_eq!(snapshot.board.live_thoughts().len(), 1);
    assert_eq!(snapshot.board.live_thoughts()[0].content, "new branch");
}

#[test]
fn capture_receipt_survives_compaction_of_its_operation() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_243_000_000);
    let mut state = session_state(&mut ids, &test_path("screenshot-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let capture = prepared_capture(&state, &mut ids, 11, 2);
    let outcome = store.commit_capture(&capture).expect("capture");
    let thought_id = apply_capture(&mut state, &capture, &outcome)
        .expect("apply capture")
        .expect("created thought");
    for index in 0..510_u64 {
        let collapsed = index % 2 == 0;
        store
            .commit(&OperationBatch::Board(BoardOperation {
                id: ids.operation_id(),
                session_id,
                sequence: OperationSequence::new(index + 2),
                kind: BoardOperationKind::Collapse,
                forward: BoardMutation::SetPresentation {
                    thought_id,
                    presentation: if collapsed {
                        ThoughtPresentation::Collapsed
                    } else {
                        ThoughtPresentation::Automatic
                    },
                },
                inverse: BoardMutation::SetPresentation {
                    thought_id,
                    presentation: if collapsed {
                        ThoughtPresentation::Automatic
                    } else {
                        ThoughtPresentation::Collapsed
                    },
                },
                created_at: Timestamp::from_millis(i64::try_from(index + 3).expect("timestamp")),
            }))
            .expect("collapse commit");
    }

    store.compact_session(session_id).expect("compact capture");
    assert!(matches!(
        store.commit_capture(&capture).expect("global dedupe"),
        CaptureCommitOutcome::AlreadyCaptured(_)
    ));
}

#[test]
fn capture_receipt_survives_trash_and_prune() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_244_000_000);
    let state = session_state(&mut ids, &test_path("screenshot-prune"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let capture = prepared_capture(&state, &mut ids, 12, 2);
    store.commit_capture(&capture).expect("capture");

    store
        .trash_session(session_id, Timestamp::from_millis(3))
        .expect("trash captured session");
    store
        .prune_session(session_id)
        .expect("prune captured session");
    assert!(matches!(
        store.commit_capture(&capture).expect("global dedupe"),
        CaptureCommitOutcome::AlreadyCaptured(_)
    ));
}

#[test]
fn version_seven_receipts_migrate_without_ownership_foreign_keys() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_245_000_000);
    let state = session_state(&mut ids, &test_path("screenshot-v7-migration"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let capture = prepared_capture(&state, &mut ids, 13, 2);
    store.commit_capture(&capture).expect("capture");
    drop(store);

    let connection = Connection::open(&fixture.config.database_path).expect("version seven DB");
    connection
        .execute_batch(
            "ALTER TABLE screenshot_capture_receipts
                 RENAME TO screenshot_capture_receipts_current;
             CREATE TABLE screenshot_capture_receipts (
                 source_fingerprint BLOB PRIMARY KEY CHECK (length(source_fingerprint) = 32),
                 session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE RESTRICT,
                 thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE RESTRICT,
                 operation_id BLOB NOT NULL REFERENCES board_operations(id) ON DELETE RESTRICT,
                 accepted_at INTEGER NOT NULL,
                 UNIQUE(operation_id)
             ) STRICT;
             INSERT INTO screenshot_capture_receipts
             SELECT * FROM screenshot_capture_receipts_current;
             DROP TABLE screenshot_capture_receipts_current;
             DROP TABLE onboarding_state;
             DELETE FROM migration_history WHERE version IN (8, 9, 10, 11);
             UPDATE schema_meta SET schema_version = 7, storage_protocol = 7;",
        )
        .expect("legacy restrictive schema");
    drop(connection);

    let mut store = fixture.open();
    let connection = Connection::open(&fixture.config.database_path).expect("migrated DB");
    let foreign_keys: i64 = connection
        .query_row(
            "SELECT count(*) FROM pragma_foreign_key_list('screenshot_capture_receipts')",
            [],
            |row| row.get(0),
        )
        .expect("foreign key count");
    assert_eq!(foreign_keys, 0);
    drop(connection);
    store
        .trash_session(session_id, Timestamp::from_millis(3))
        .expect("trash migrated capture");
    store
        .prune_session(session_id)
        .expect("prune migrated capture");
    assert!(matches!(
        store.commit_capture(&capture).expect("migrated dedupe"),
        CaptureCommitOutcome::AlreadyCaptured(_)
    ));
}
