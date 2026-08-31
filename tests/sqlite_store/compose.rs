use super::*;

#[test]
fn empty_resume_enters_compose_without_consuming_a_durable_sequence() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let state = session_state(&mut ids, &test_path("proqi-empty-compose"));
    let session_id = state.board.session.id;
    let sequence = state.board.session.last_durable_sequence;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create empty session");
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened
        .load_session(session_id)
        .expect("resume empty session");
    assert_eq!(snapshot.board.session.last_durable_sequence, sequence);
    assert!(snapshot.board_operations.is_empty());
    assert!(snapshot.revisions.is_empty());
    let restored = AppState::from_snapshot(snapshot).expect("rehydrate empty session");
    assert_eq!(restored.mode, proqi::application::InteractionMode::Compose);
    assert!(restored.board.live_thoughts().is_empty());
    assert!(restored.board_history().is_empty());
}

#[test]
fn first_populated_create_round_trips_as_one_restart_safe_undo_unit() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-compose-first-input"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "nqs:?jk界", 2);
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened
        .load_session(session_id)
        .expect("resume populated session");
    assert_eq!(snapshot.board_operations.len(), 1);
    assert!(snapshot.revisions.is_empty());
    let mut restored = AppState::from_snapshot(snapshot).expect("rehydrate populated session");
    assert_eq!(
        restored.board.thought(thought_id).expect("thought").content,
        "nqs:?jk界"
    );
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut reopened, &undo);
    drop(reopened);

    let mut reopened = fixture.open();
    let snapshot = reopened
        .load_session(session_id)
        .expect("resume undone session");
    assert!(snapshot.board.live_thoughts().is_empty());
    let mut restored = AppState::from_snapshot(snapshot).expect("rehydrate undone session");
    assert_eq!(restored.mode, proqi::application::InteractionMode::Compose);
    let redo = one_effect(
        &mut restored,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut reopened, &redo);
    let resumed = reopened
        .load_session(session_id)
        .expect("resume redone session");
    assert_eq!(
        resumed
            .board
            .thought(thought_id)
            .expect("redone thought")
            .content,
        "nqs:?jk界"
    );
}
