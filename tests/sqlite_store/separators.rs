use super::*;
use proqi::domain::BoardItemId;

fn live_ids(snapshot: &proqi::ports::store::SessionSnapshot) -> Vec<BoardItemId> {
    snapshot
        .board
        .live_items()
        .into_iter()
        .map(proqi::domain::BoardItemRef::id)
        .collect()
}

fn assert_separator_timestamp(
    snapshot: &proqi::ports::store::SessionSnapshot,
    separator_id: proqi::domain::SeparatorId,
) {
    let separator = snapshot.board.separator(separator_id).expect("separator");
    assert_eq!(separator.created_at, Timestamp::from_millis(3));
    assert_eq!(separator.updated_at, Timestamp::from_millis(3));
}

#[test]
fn separator_identity_order_and_history_survive_restarts() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_995_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-separator-restart"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first", 2);
    let separator = ids.separator_id();
    let insert = one_effect(
        &mut state,
        Action::InsertSeparator {
            separator_id: separator,
            operation_id: ids.operation_id(),
            insertion_index: 1,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &insert);
    let second = create_thought(&mut store, &mut state, &mut ids, "second", 4);
    assert_eq!(
        state
            .board
            .live_items()
            .into_iter()
            .map(proqi::domain::BoardItemRef::id)
            .collect::<Vec<_>>(),
        vec![first.into(), separator.into(), second.into()]
    );
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("first restart");
    assert_eq!(
        live_ids(&snapshot),
        vec![first.into(), separator.into(), second.into()]
    );
    assert_separator_timestamp(&snapshot, separator);
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate");

    let moved = one_effect(
        &mut state,
        Action::MoveItem {
            operation_id: ids.operation_id(),
            item_id: separator.into(),
            to: 0,
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut reopened, &moved);
    let deleted = one_effect(
        &mut state,
        Action::DeleteItems {
            operation_id: ids.operation_id(),
            item_ids: vec![separator.into()],
            kind: BoardOperationKind::Delete,
            at: Timestamp::from_millis(6),
        },
    );
    persist_effect(&mut reopened, &deleted);
    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(7),
        },
    );
    persist_effect(&mut reopened, &undo);
    drop(reopened);

    for _ in 0..2 {
        let mut reopened = fixture.open();
        let snapshot = reopened.load_session(session_id).expect("repeat restart");
        assert_eq!(
            live_ids(&snapshot),
            vec![separator.into(), first.into(), second.into()]
        );
        assert_eq!(snapshot.board_history_cursor, 4);
        assert_eq!(snapshot.board_operations.len(), 5);
    }
}

#[test]
fn consecutive_and_edge_separators_round_trip_without_coalescing() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_996_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-separator-edges"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let mut separator_ids = Vec::new();
    for (insertion_index, at) in [(0, 2), (1, 3), (0, 4)] {
        let separator_id = ids.separator_id();
        separator_ids.push(separator_id);
        let effect = one_effect(
            &mut state,
            Action::InsertSeparator {
                separator_id,
                operation_id: ids.operation_id(),
                insertion_index,
                at: Timestamp::from_millis(at),
            },
        );
        persist_effect(&mut store, &effect);
    }
    drop(store);
    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("restart");
    assert_eq!(snapshot.board.live_separators().len(), 3);
    assert_eq!(
        live_ids(&snapshot),
        vec![
            separator_ids[2].into(),
            separator_ids[0].into(),
            separator_ids[1].into()
        ]
    );
}
