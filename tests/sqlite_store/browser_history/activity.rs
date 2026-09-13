use super::*;

fn restored_session_with_trash_redo(
    fixture: &DatabaseFixture,
) -> (SqliteStore, AppState, FakeIdGenerator, ThoughtId) {
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_050_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-browser-activity"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "seed", 2);
    let previous_activity = state.board.session.last_active_at;
    let trash = BrowserOperation::trash(
        ids.operation_id(),
        session_id,
        previous_activity,
        Timestamp::from_millis(10),
    );
    store
        .commit_browser_operation(&trash)
        .expect("trash session");
    move_history(
        &mut store,
        ids.operation_id(),
        true,
        Timestamp::from_millis(11),
    )
    .expect("undo trash");
    assert_eq!(
        store.browser_history_status().expect("trash redo").redo,
        Some(history_entry(&trash))
    );
    let snapshot = store.load_session(session_id).expect("restored session");
    (
        store,
        AppState::from_snapshot(snapshot).expect("rehydrate restored session"),
        ids,
        thought_id,
    )
}

#[test]
fn board_activity_invalidates_a_conflicting_trash_redo() {
    let fixture = DatabaseFixture::new();
    let (mut store, mut state, mut ids, thought_id) = restored_session_with_trash_redo(&fixture);
    let effect = one_effect(
        &mut state,
        Action::SetPresentation {
            operation_id: ids.operation_id(),
            thought_id,
            presentation: ThoughtPresentation::Collapsed,
            at: Timestamp::from_millis(20),
        },
    );

    persist_effect(&mut store, &effect);

    assert_eq!(store.browser_history_status().expect("status").redo, None);
}

#[test]
fn editor_activity_invalidates_a_conflicting_trash_redo() {
    let fixture = DatabaseFixture::new();
    let (mut store, mut state, mut ids, thought_id) = restored_session_with_trash_redo(&fixture);
    let effect = one_effect(
        &mut state,
        Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: "seed".to_owned(),
            after_content: "edited".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 4),
            after_cursor: TextPosition::new(0, 6),
            at: Timestamp::from_millis(20),
        },
    );

    persist_effect(&mut store, &effect);

    assert_eq!(store.browser_history_status().expect("status").redo, None);
}

#[test]
fn equal_timestamp_open_still_invalidates_a_conflicting_trash_redo() {
    let fixture = DatabaseFixture::new();
    let (mut store, state, _, _) = restored_session_with_trash_redo(&fixture);
    let session_id = state.board.session.id;

    store
        .record_session_open(
            session_id,
            &test_path("proqi-browser-equal-activity"),
            state.board.session.last_active_at,
        )
        .expect("same-millisecond activity");

    assert_eq!(store.browser_history_status().expect("status").redo, None);
}

#[test]
fn later_activity_preserves_nonconflicting_rename_redo() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_050_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    let operation = rename(&mut ids, session_id, "first", "renamed");
    store
        .commit_browser_operation(&operation)
        .expect("rename session");
    move_history(
        &mut store,
        ids.operation_id(),
        true,
        Timestamp::from_millis(3),
    )
    .expect("undo rename");

    store
        .record_session_open(
            session_id,
            &test_path("proqi-browser-rename-redo"),
            Timestamp::from_millis(20),
        )
        .expect("record later activity");

    assert_eq!(
        store.browser_history_status().expect("rename redo").redo,
        Some(history_entry(&operation))
    );
}
