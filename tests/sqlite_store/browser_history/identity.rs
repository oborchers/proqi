use super::*;

#[test]
fn corrupt_browser_metadata_is_rejected_before_history_moves() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    store
        .commit_browser_operation(&rename(&mut ids, session_id, "first", "second"))
        .expect("rename");
    drop(store);
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("database");
    connection
        .execute("UPDATE browser_operations SET kind = 'trash'", [])
        .expect("corrupt metadata");
    drop(connection);

    let mut reopened = fixture.open();
    assert!(matches!(
        reopened.browser_history_status(),
        Err(StoreError::Corrupt(message))
            if message == "Browser operation metadata does not match its payload"
    ));
}

#[test]
fn browser_operation_and_history_request_ids_share_one_namespace() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    let original = rename(&mut ids, session_id, "first", "second");
    store
        .commit_browser_operation(&original)
        .expect("initial rename");
    let history_request_id = ids.operation_id();
    move_history(
        &mut store,
        history_request_id,
        true,
        Timestamp::from_millis(3),
    )
    .expect("undo");

    let reused_request = BrowserOperation::rename(
        history_request_id,
        session_id,
        Some("first".to_owned()),
        Some("third".to_owned()),
        Timestamp::from_millis(4),
    )
    .expect("candidate rename");
    assert!(matches!(
        store.commit_browser_operation(&reused_request),
        Err(StoreError::Conflict(message))
            if message == "operation identity is already used by Browser history"
    ));
    assert!(matches!(
        store.move_browser_history(
            original.id(),
            history_entry(&original),
            false,
            Timestamp::from_millis(5)
        ),
        Err(StoreError::Conflict(message))
            if message == "operation identity is already used by Browser history"
    ));
}

#[test]
fn browser_ids_cannot_be_reused_by_later_session_history() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let state = session_state(&mut ids, &test_path("proqi-browser-id-namespace"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let browser_operation = BrowserOperation::rename(
        ids.operation_id(),
        session_id,
        None,
        Some("named".to_owned()),
        Timestamp::from_millis(2),
    )
    .expect("Browser rename");
    store
        .commit_browser_operation(&browser_operation)
        .expect("Browser operation");

    assert_board_id_conflicts(
        &mut store,
        state.clone(),
        &mut ids,
        browser_operation.id(),
        "collision",
        Timestamp::from_millis(3),
    );

    let history_request_id = ids.operation_id();
    move_history(
        &mut store,
        history_request_id,
        true,
        Timestamp::from_millis(4),
    )
    .expect("Browser undo");
    assert_board_id_conflicts(
        &mut store,
        state,
        &mut ids,
        history_request_id,
        "second collision",
        Timestamp::from_millis(5),
    );
    assert!(
        store
            .load_session(session_id)
            .expect("no colliding thought committed")
            .board
            .thoughts()
            .is_empty()
    );
}

fn assert_board_id_conflicts(
    store: &mut SqliteStore,
    mut state: AppState,
    ids: &mut FakeIdGenerator,
    operation_id: proqi::domain::OperationId,
    content: &str,
    at: Timestamp,
) {
    let effect = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id,
            content: content.to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at,
        },
    );
    let Effect::CommitBoardOperation(operation) = effect else {
        panic!("board effect")
    };
    assert!(matches!(
        store.commit(&OperationBatch::Board(operation)),
        Err(StoreError::Conflict(message))
            if message == "durable identity is already used by Browser history"
    ));
}
