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
fn no_op_rename_receipt_survives_restart_and_reserves_its_identity() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_500_000);
    let session_id = create_session(&mut store, &mut ids, "stable");
    let operation_id = ids.operation_id();
    let first = store
        .commit_browser_noop_rename(
            operation_id,
            session_id,
            Some("stable"),
            Timestamp::from_millis(2),
        )
        .expect("reserve no-op rename");
    assert!(!first.idempotent_replay);
    assert_eq!(
        store.browser_history_status().expect("history status"),
        BrowserHistoryStatus::default(),
        "a no-op receipt must not create history"
    );
    drop(store);

    let mut reopened = fixture.open();
    let replay = reopened
        .commit_browser_noop_rename(
            operation_id,
            session_id,
            Some("stable"),
            Timestamp::from_millis(3),
        )
        .expect("replay no-op rename after restart");
    assert!(replay.idempotent_replay);
    assert!(matches!(
        reopened.browser_operation(operation_id),
        Err(StoreError::Conflict(message))
            if message == "operation identity is already used by Browser history"
    ));
    let changed = BrowserOperation::rename(
        operation_id,
        session_id,
        Some("stable".to_owned()),
        Some("changed".to_owned()),
        Timestamp::from_millis(4),
    )
    .expect("changed rename");
    assert!(matches!(
        reopened.commit_browser_operation(&changed),
        Err(StoreError::Conflict(message))
            if message == "Browser operation identity was reused for different content"
    ));
}

#[test]
fn stale_no_op_rename_does_not_reserve_its_identity() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_750_000);
    let session_id = create_session(&mut store, &mut ids, "local");
    store
        .commit_browser_operation(&rename(&mut ids, session_id, "local", "durable"))
        .expect("concurrent durable rename");
    let operation_id = ids.operation_id();

    assert!(matches!(
        store.commit_browser_noop_rename(
            operation_id,
            session_id,
            Some("local"),
            Timestamp::from_millis(3),
        ),
        Err(StoreError::Conflict(message))
            if message == "session name changed before Browser receipt commit"
    ));
    let receipt = store
        .commit_browser_noop_rename(
            operation_id,
            session_id,
            Some("durable"),
            Timestamp::from_millis(4),
        )
        .expect("reserve identity after refreshing the durable name");
    assert!(!receipt.idempotent_replay);
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
fn browser_lookup_rejects_ids_owned_by_session_or_history_move_receipts() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_001_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-browser-lookup-namespace"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");

    let session_operation_id = ids.operation_id();
    let thought_id = ids.thought_id();
    let effect = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: session_operation_id,
            content: "session history".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    persist_effect(&mut store, &effect);
    assert!(matches!(
        store.browser_operation(session_operation_id),
        Err(StoreError::Conflict(message))
            if message == "operation identity is already used by session history"
    ));

    let rename = BrowserOperation::rename(
        ids.operation_id(),
        session_id,
        None,
        Some("renamed".to_owned()),
        Timestamp::from_millis(3),
    )
    .expect("rename");
    store
        .commit_browser_operation(&rename)
        .expect("Browser rename");
    assert!(
        store
            .commit_browser_noop_rename(
                rename.id(),
                session_id,
                Some("renamed"),
                Timestamp::from_millis(4),
            )
            .expect("effectful rename replay through no-op owner path")
            .idempotent_replay
    );
    let move_id = ids.operation_id();
    move_history(&mut store, move_id, true, Timestamp::from_millis(5)).expect("Browser undo");
    assert!(matches!(
        store.browser_operation(move_id),
        Err(StoreError::Conflict(message))
            if message == "operation identity is already used by Browser history"
    ));
    assert_eq!(
        store.browser_operation(rename.id()).expect("rename replay"),
        Some(rename)
    );
    assert_eq!(
        store
            .browser_operation(ids.operation_id())
            .expect("unused identity"),
        None
    );
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
