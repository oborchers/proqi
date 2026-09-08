//! Identical snapshots from different retained history steps are distinct occurrences.
use super::*;

fn split_then_reinsert(
    store: &mut proqi::adapters::sqlite::SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
) -> (ThoughtId, ThoughtId) {
    let source = create(store, state, ids, true);
    let before = state.board.thought(source).expect("source").clone();
    let right = ids.thought_id();
    apply(
        store,
        state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: right,
            operation_id: ids.operation_id(),
            expected_content: before.content.clone(),
            expected_annotations: before.annotations.clone(),
            source_content: before.content.clone(),
            source_annotations: before.annotations,
            at_byte: 0,
            at: Timestamp::from_millis(3),
        },
    );
    apply(
        store,
        state,
        Action::EditThought {
            thought_id: source,
            revision_id: ids.revision_id(),
            before_content: String::new(),
            before_annotations: Vec::new(),
            after_content: before.content.clone(),
            after_annotations: vec![annotation(0, before.content.len(), true)],
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 0),
            at: Timestamp::from_millis(4),
        },
    );
    (source, right)
}

#[test]
fn legacy_split_then_identical_reinsertion_migrates_and_replays_without_aliasing_live_anchors() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("repeated-snapshot"));
    let session = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let (source, right) = split_then_reinsert(&mut store, &mut state, &mut ids);
    assert_eq!((ordinal(&state, source), ordinal(&state, right)), (2, 1));
    drop(store);
    downgrade_to_legacy(&fixture);
    let mut store = fixture.open();
    let mut state =
        AppState::from_snapshot(store.load_session(session).expect("migration")).expect("state");
    assert_eq!((ordinal(&state, source), ordinal(&state, right)), (1, 2));
    for undo in [true, false, true, false] {
        let operation_id = ids.operation_id();
        let scope = UndoScope::Editor { thought_id: source };
        let at = Timestamp::from_millis(5);
        let action = if undo {
            Action::Undo {
                operation_id,
                scope,
                at,
            }
        } else {
            Action::Redo {
                operation_id,
                scope,
                at,
            }
        };
        apply(&mut store, &mut state, action);
        assert_eq!(ordinal(&state, right), 2);
        if undo {
            assert!(
                state
                    .board
                    .thought(source)
                    .expect("source")
                    .annotations
                    .is_empty()
            );
        } else {
            assert_eq!(ordinal(&state, source), 1);
        }
    }
    drop(store);
    let mut store = fixture.open();
    let mut restored =
        AppState::from_snapshot(store.load_session(session).expect("restart")).expect("state");
    assert_eq!(restored.board.thoughts(), state.board.thoughts());
    let next = create(&mut store, &mut restored, &mut ids, true);
    assert_eq!(ordinal(&restored, next), 3);
}

#[test]
fn legacy_migration_anchors_undone_split_before_dormant_identical_editor_redo() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("undone-repeated-snapshot"));
    let session = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let (source, right) = split_then_reinsert(&mut store, &mut state, &mut ids);
    for scope in [UndoScope::Editor { thought_id: source }, UndoScope::Board] {
        apply(
            &mut store,
            &mut state,
            Action::Undo {
                operation_id: ids.operation_id(),
                scope,
                at: Timestamp::from_millis(5),
            },
        );
    }
    drop(store);
    downgrade_to_legacy(&fixture);
    let mut store = fixture.open();
    let mut state =
        AppState::from_snapshot(store.load_session(session).expect("migration")).expect("state");
    assert_eq!(ordinal(&state, source), 1);
    assert_eq!(state.board.attachment_counters().image(), 2);
    for scope in [UndoScope::Board, UndoScope::Editor { thought_id: source }] {
        apply(
            &mut store,
            &mut state,
            Action::Redo {
                operation_id: ids.operation_id(),
                scope,
                at: Timestamp::from_millis(6),
            },
        );
    }
    assert_eq!((ordinal(&state, source), ordinal(&state, right)), (2, 1));
    drop(store);
    let mut store = fixture.open();
    let mut restored =
        AppState::from_snapshot(store.load_session(session).expect("restart")).expect("state");
    assert_eq!(restored.board.thoughts(), state.board.thoughts());
    let next = create(&mut store, &mut restored, &mut ids, true);
    assert_eq!(ordinal(&restored, next), 3);
}
