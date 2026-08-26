use super::*;

#[test]
fn bulk_delete_is_atomic_and_restored_by_one_persistent_undo() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_300_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-bulk-delete"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "second", 3);

    let deletion = one_effect(
        &mut state,
        Action::DeleteThoughts {
            operation_id: ids.operation_id(),
            thought_ids: vec![first, second],
            kind: BoardOperationKind::Delete,
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut store, &deletion);
    assert!(
        store
            .load_session(session_id)
            .expect("deleted snapshot")
            .board
            .live_thoughts()
            .is_empty()
    );

    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut store, &undo);
    let restored = store.load_session(session_id).expect("restored snapshot");
    let live = restored.board.live_thoughts();
    assert_eq!(live.len(), 2);
    assert_eq!(live[0].content, "first");
    assert_eq!(live[1].content, "second");
}
