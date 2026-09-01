use super::*;

#[test]
fn dirty_editor_split_is_one_restart_safe_sqlite_operation() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-dirty-split"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let source = create_thought(&mut store, &mut state, &mut ids, "base", 2);
    let neighbor = ids.thought_id();
    persist_action(
        &mut store,
        &mut state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: neighbor,
            operation_id: ids.operation_id(),
            expected_content: "base".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "base dirty".to_owned(),
            source_annotations: Vec::new(),
            at_byte: 4,
            at: Timestamp::from_millis(3),
        },
    );
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen split");
    assert_eq!(live_contents(&snapshot), ["base", " dirty"]);
    assert!(snapshot.revisions.is_empty(), "no separate editor revision");
    let mut restored = AppState::from_snapshot(snapshot).expect("rehydrate split");
    persist_board_history(&mut reopened, &mut restored, &mut ids, true, 4);
    drop(reopened);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen undo");
    assert_eq!(live_contents(&snapshot), ["base"]);
    assert!(snapshot.revisions.is_empty());
    let mut restored = AppState::from_snapshot(snapshot).expect("rehydrate undo");
    persist_board_history(&mut reopened, &mut restored, &mut ids, false, 5);
    drop(reopened);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen redo");
    assert_eq!(live_contents(&snapshot), ["base", " dirty"]);
    assert!(snapshot.revisions.is_empty());
}
