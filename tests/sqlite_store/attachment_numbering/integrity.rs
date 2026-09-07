//! Durable allocation rejects stale writers and malformed stored identity.
use super::*;
use proqi::ports::store::StoreError;

#[test]
fn stale_creators_cannot_commit_duplicate_ordinals_and_reload_allocates_next() {
    let fixture = DatabaseFixture::new();
    let mut first = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("numbering-writers"));
    let session = state.board.session.id;
    first
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let mut second = fixture.open();
    let mut contender =
        AppState::from_snapshot(second.load_session(session).expect("snapshot")).expect("state");
    let winner = create(&mut first, &mut state, &mut ids, true);
    let effects = reduce(
        &mut contender,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content: "/offline/second.png".to_owned(),
            annotations: vec![annotation(0, "/offline/second.png".len(), true)],
            insertion_index: None,
            at: Timestamp::from_millis(3),
        },
    )
    .expect("stale proposal");
    let batch = effects[0].persistence_batch().expect("batch");
    assert!(matches!(
        second.commit(&batch),
        Err(StoreError::Conflict(_))
    ));
    let mut reloaded =
        AppState::from_snapshot(second.load_session(session).expect("reload")).expect("state");
    assert_eq!(ordinal(&reloaded, winner), 1);
    let next = create(&mut second, &mut reloaded, &mut ids, true);
    assert_eq!(ordinal(&reloaded, next), 2);
    assert_eq!(
        first
            .load_session(session)
            .expect("durable")
            .board
            .live_thoughts()
            .len(),
        2
    );
}

#[test]
fn missing_zero_duplicate_and_underreported_ordinals_are_corrupt() {
    for corruption in [
        "UPDATE thoughts SET annotations_json = json_remove(annotations_json, '$[0].kind.ordinal')",
        "UPDATE thoughts SET annotations_json = json_set(annotations_json, '$[0].kind.ordinal', 0)",
        "UPDATE thoughts SET annotations_json = json_set(annotations_json, '$[0].kind.ordinal', 1)",
        "UPDATE sessions SET attachment_image_high = 1",
        "UPDATE board_operations SET payload_json = json_remove(payload_json, '$.forward.thought.annotations[0].kind.ordinal')",
    ] {
        let fixture = DatabaseFixture::new();
        let mut store = fixture.open();
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let mut state = session_state(&mut ids, &test_path("numbering-corrupt"));
        let session = state.board.session.id;
        store
            .commit(&OperationBatch::CreateSession(state.board.session.clone()))
            .expect("session");
        create(&mut store, &mut state, &mut ids, true);
        create(&mut store, &mut state, &mut ids, true);
        rusqlite::Connection::open(&fixture.config.database_path)
            .expect("database")
            .execute_batch(corruption)
            .expect("corruption fixture");
        assert!(
            matches!(store.load_session(session), Err(StoreError::Corrupt(_))),
            "{corruption}"
        );
    }
}
