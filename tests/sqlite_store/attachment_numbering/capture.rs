//! Screenshot sequence allocation, atomic receipt rollback and lost acknowledgement replay.

use super::{DatabaseFixture, create, ordinal, session_state, test_path};
use proqi::{
    adapters::memory::FakeIdGenerator,
    application::{apply_capture, prepare_capture},
    domain::Timestamp,
    ports::{
        environment::IdGenerator,
        screenshot::{ScreenshotCandidate, ScreenshotFingerprint, ScreenshotImageType},
        store::{CaptureCommitOutcome, OperationBatch, Store},
    },
};

fn candidate(index: u8) -> ScreenshotCandidate {
    ScreenshotCandidate {
        path: test_path(&format!("capture Grüße {index}.png")),
        fingerprint: ScreenshotFingerprint([index; 32]),
        image_type: ScreenshotImageType::Png,
    }
}

#[test]
fn screenshot_sequence_shares_images_with_paste_and_keeps_file_sequence_independent() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("capture-numbering"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    for index in 1..=16 {
        let commit = prepare_capture(
            &state,
            &candidate(index),
            ids.thought_id(),
            ids.operation_id(),
            Timestamp::from_millis(3),
        )
        .expect("capture");
        let outcome = store.commit_capture(&commit).expect("commit");
        let thought = apply_capture(&mut state, &commit, &outcome)
            .expect("apply")
            .expect("created");
        assert_eq!(ordinal(&state, thought), u64::from(index));
        let replay = store.commit_capture(&commit).expect("replay");
        assert!(matches!(replay, CaptureCommitOutcome::AlreadyCaptured(_)));
        assert_eq!(
            apply_capture(&mut state, &commit, &replay).expect("already applied"),
            None
        );
    }
    let file = create(&mut store, &mut state, &mut ids, false);
    assert_eq!(ordinal(&state, file), 1);
    let pasted = create(&mut store, &mut state, &mut ids, true);
    assert_eq!(ordinal(&state, pasted), 17);
    assert_eq!(
        store
            .load_session(state.board.session.id)
            .expect("snapshot")
            .board
            .attachment_counters()
            .image(),
        17
    );
}

#[test]
fn failed_receipt_rolls_back_number_and_lost_acknowledgement_recovers_exact_proposal() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("capture-retry"));
    let session = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("database");
    connection.execute_batch("CREATE TRIGGER fail_capture BEFORE INSERT ON screenshot_capture_receipts BEGIN SELECT RAISE(ABORT, 'capture receipt failure'); END;").expect("inject failure");
    let commit = prepare_capture(
        &state,
        &candidate(1),
        ids.thought_id(),
        ids.operation_id(),
        Timestamp::from_millis(3),
    )
    .expect("proposal");
    assert!(store.commit_capture(&commit).is_err());
    let failed = store.load_session(session).expect("rolled back");
    assert!(failed.board.thoughts().is_empty());
    assert_eq!(failed.board.attachment_counters().image(), 0);
    connection
        .execute_batch("DROP TRIGGER fail_capture;")
        .expect("remove failure");
    let _lost_acknowledgement = store.commit_capture(&commit).expect("durable retry");
    let replay = store
        .commit_capture(&commit)
        .expect("retry after lost acknowledgement");
    let thought = apply_capture(&mut state, &commit, &replay)
        .expect("recover receipt")
        .expect("apply once");
    assert_eq!(ordinal(&state, thought), 1);
    let next = prepare_capture(
        &state,
        &candidate(2),
        ids.thought_id(),
        ids.operation_id(),
        Timestamp::from_millis(4),
    )
    .expect("next");
    let outcome = store.commit_capture(&next).expect("next commit");
    let thought = apply_capture(&mut state, &next, &outcome)
        .expect("apply")
        .expect("created");
    assert_eq!(ordinal(&state, thought), 2);
}
