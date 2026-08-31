//! Restart-safe persistence and history for insertion before the first thought.

use super::*;

#[test]
fn top_blank_persists_and_repeated_undo_redo_keeps_exact_order() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-top-boundary"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "second", 3);
    let blank = ids.thought_id();
    let effect = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id: blank,
            operation_id: ids.operation_id(),
            content: String::new(),
            annotations: Vec::new(),
            insertion_index: Some(0),
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut store, &effect);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen top blank");
    assert_eq!(
        snapshot
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>(),
        [blank, first, second]
    );
    let mut restored = AppState::from_snapshot(snapshot).expect("restore history");
    for undo in [true, false, true, false] {
        let action = if undo {
            Action::Undo {
                operation_id: ids.operation_id(),
                scope: UndoScope::Board,
                at: Timestamp::from_millis(5),
            }
        } else {
            Action::Redo {
                operation_id: ids.operation_id(),
                scope: UndoScope::Board,
                at: Timestamp::from_millis(6),
            }
        };
        let effect = one_effect(&mut restored, action);
        persist_effect(&mut store, &effect);
    }
    drop(store);

    let order = fixture
        .open()
        .load_session(session_id)
        .expect("reopen redone top blank")
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.id)
        .collect::<Vec<_>>();
    assert_eq!(order, [blank, first, second]);
}
