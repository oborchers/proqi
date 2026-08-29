use super::*;

#[test]
fn list_indentation_undo_and_redo_survive_reopen() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-list-indent-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "- parent\r\n- child", 2);
    let edit = one_effect(
        &mut state,
        Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: "- parent\r\n- child".to_owned(),
            after_content: "- parent\r\n  - child".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(1, 7),
            after_cursor: TextPosition::new(1, 9),
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &edit);
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened
        .load_session(session_id)
        .expect("reopen indentation");
    assert_eq!(
        snapshot.board.thought(thought_id).expect("thought").content,
        "- parent\r\n  - child"
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate indentation");
    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut reopened, &undo);
    drop(reopened);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen undo");
    assert_eq!(
        snapshot.board.thought(thought_id).expect("thought").content,
        "- parent\r\n- child"
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate undo");
    let redo = one_effect(
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut reopened, &redo);
    drop(reopened);

    let redone = fixture
        .open()
        .load_session(session_id)
        .expect("reopen redo");
    assert_eq!(
        redone.board.thought(thought_id).expect("thought").content,
        "- parent\r\n  - child"
    );
}
