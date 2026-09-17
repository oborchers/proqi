use super::*;

#[test]
fn names_survive_restart_and_board_undo_redo_without_changing_body() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-thought-name"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "exact body", 2);
    let name = proqi::domain::ThoughtName::new("Unicode 設計").expect("name");
    let effect = one_effect(
        &mut state,
        Action::RenameThought {
            operation_id: ids.operation_id(),
            thought_id,
            name: Some(name.clone()),
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &effect);

    let reopened = fixture
        .open()
        .load_session(session_id)
        .expect("restart load");
    let thought = reopened.board.thought(thought_id).expect("thought");
    assert_eq!(thought.content, "exact body");
    assert_eq!(thought.name.as_ref(), Some(&name));

    for (undo, expected) in [(true, None), (false, Some(name.clone()))] {
        let effect = one_effect(
            &mut state,
            if undo {
                Action::Undo {
                    operation_id: ids.operation_id(),
                    scope: UndoScope::Board,
                    at: Timestamp::from_millis(4),
                }
            } else {
                Action::Redo {
                    operation_id: ids.operation_id(),
                    scope: UndoScope::Board,
                    at: Timestamp::from_millis(5),
                }
            },
        );
        persist_effect(&mut store, &effect);
        let loaded = store.load_session(session_id).expect("history load");
        let thought = loaded.board.thought(thought_id).expect("thought");
        assert_eq!(thought.content, "exact body");
        assert_eq!(thought.name, expected);
    }
}

#[test]
fn same_value_rename_receipt_is_retry_safe_without_a_history_unit() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_100_000);
    let mut state = session_state(&mut ids, &test_path("proqi-thought-name-noop"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "body", 2);
    let effect = one_effect(
        &mut state,
        Action::RenameThought {
            operation_id: ids.operation_id(),
            thought_id,
            name: None,
            at: Timestamp::from_millis(3),
        },
    );
    let batch = effect.persistence_batch().expect("durable batch");
    let first = store
        .commit(&batch)
        .expect("first commit")
        .expect("receipt");
    let second = store.commit(&batch).expect("retry").expect("receipt");
    assert!(!first.idempotent_replay);
    assert!(second.idempotent_replay);
    let cursor: u32 = rusqlite::Connection::open(&fixture.config.database_path)
        .expect("database")
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("cursor");
    assert_eq!(cursor, 1);
}
