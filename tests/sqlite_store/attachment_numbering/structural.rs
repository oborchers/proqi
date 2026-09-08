//! Movement lineage survives legacy synthesis and current undo history.
use super::*;

#[test]
fn split_occurrences_keep_identity_through_restart_and_legacy_history_synthesis() {
    for legacy in [false, true] {
        let fixture = DatabaseFixture::new();
        let mut store = fixture.open();
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let mut state = session_state(&mut ids, &test_path("numbering-split"));
        let session = state.board.session.id;
        store
            .commit(&OperationBatch::CreateSession(state.board.session.clone()))
            .expect("session");
        let source = ids.thought_id();
        let path = "/offline/same.png";
        let content = format!("{path}\n{path}");
        apply(
            &mut store,
            &mut state,
            Action::CreateThought {
                thought_id: source,
                operation_id: ids.operation_id(),
                content: content.clone(),
                annotations: vec![
                    annotation(0, path.len(), true),
                    annotation(path.len() + 1, content.len(), true),
                ],
                insertion_index: None,
                at: Timestamp::from_millis(2),
            },
        );
        let annotations = state
            .board
            .thought(source)
            .expect("source")
            .annotations
            .clone();
        let right = ids.thought_id();
        apply(
            &mut store,
            &mut state,
            Action::SplitThought {
                thought_id: source,
                new_thought_id: right,
                operation_id: ids.operation_id(),
                expected_content: content.clone(),
                expected_annotations: annotations.clone(),
                source_content: content.clone(),
                source_annotations: annotations,
                at_byte: path.len() + 1,
                at: Timestamp::from_millis(3),
            },
        );
        assert_eq!((ordinal(&state, source), ordinal(&state, right)), (1, 2));
        drop(store);
        if legacy {
            downgrade_to_legacy(&fixture);
        }
        let mut store = fixture.open();
        let mut state =
            AppState::from_snapshot(store.load_session(session).expect("snapshot")).expect("state");
        assert_eq!((ordinal(&state, source), ordinal(&state, right)), (1, 2));
        replay_split(&mut store, &mut state, &mut ids, source, right, &content);
        let next = create(&mut store, &mut state, &mut ids, true);
        assert_eq!(ordinal(&state, next), 3);
    }
}

fn replay_split(
    store: &mut proqi::adapters::sqlite::SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    source: ThoughtId,
    right: ThoughtId,
    content: &str,
) {
    for undo in [true, false, true, false] {
        let operation_id = ids.operation_id();
        let scope = UndoScope::Board;
        let at = Timestamp::from_millis(4);
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
        apply(store, state, action);
        assert_eq!(ordinal(state, source), 1);
        assert_eq!(ordinal(state, right), 2);
        if undo {
            assert_eq!(
                state.board.thought(source).expect("source").content,
                content
            );
        }
    }
}
