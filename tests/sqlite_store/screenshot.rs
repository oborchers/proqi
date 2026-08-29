use super::*;
use proqi::{
    application::{apply_capture, prepare_capture},
    ports::{
        screenshot::{ScreenshotCandidate, ScreenshotFingerprint, ScreenshotImageType},
        store::CaptureCommitOutcome,
    },
};

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
    assert_eq!(thought.content, candidate.path.to_string_lossy());
    assert!(matches!(
        thought.annotations.as_slice(),
        [ContentAnnotation {
            start: 0,
            end,
            kind: ContentAnnotationKind::Attachment { image: true, .. },
        }] if *end == thought.content.len()
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
