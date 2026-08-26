use super::*;

#[test]
fn board_compaction_preserves_current_state_undo_and_idempotency() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = ids.thought_id();
    let create_id = ids.operation_id();
    let create = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: create_id,
            content: "retained exact content".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    persist_effect(&mut store, &create);

    for index in 0..510_u64 {
        let collapsed = index % 2 == 0;
        let sequence = OperationSequence::new(index + 2);
        let operation = BoardOperation {
            id: ids.operation_id(),
            session_id,
            sequence,
            kind: BoardOperationKind::Collapse,
            forward: BoardMutation::SetPresentation {
                thought_id,
                presentation: presentation(collapsed),
            },
            inverse: BoardMutation::SetPresentation {
                thought_id,
                presentation: presentation(!collapsed),
            },
            created_at: Timestamp::from_millis(i64::try_from(index + 3).expect("timestamp")),
        };
        store
            .commit(&OperationBatch::Board(operation))
            .expect("collapse commit");
    }

    store.compact_session(session_id).expect("compact session");
    let snapshot = store.load_session(session_id).expect("compacted snapshot");
    assert_eq!(snapshot.board_operations.len(), 500);
    assert_eq!(snapshot.board_history_cursor, 500);
    assert_eq!(
        snapshot.board.thought(thought_id).expect("thought").content,
        "retained exact content"
    );
    assert!(matches!(
        store
            .operation_request(create_id)
            .expect("operation lookup"),
        Some(proqi::ports::store::StoredOperationRequest::Compacted {
            replay: proqi::ports::store::CompactedOperationRequest::Add { .. },
            ..
        })
    ));

    let mut restored = AppState::from_snapshot(snapshot).expect("restored state");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(600),
        },
    );
    persist_effect(&mut store, &undo);
    let after_undo = store.load_session(session_id).expect("undo snapshot");
    assert_eq!(after_undo.board_history_cursor, 499);
}

#[test]
fn compaction_preserves_every_redo_entry() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_100_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-redo-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "thought", 2);
    for index in 0..504_u64 {
        let collapsed = index % 2 == 0;
        store
            .commit(&OperationBatch::Board(BoardOperation {
                id: ids.operation_id(),
                session_id,
                sequence: OperationSequence::new(index + 2),
                kind: BoardOperationKind::Collapse,
                forward: BoardMutation::SetPresentation {
                    thought_id,
                    presentation: presentation(collapsed),
                },
                inverse: BoardMutation::SetPresentation {
                    thought_id,
                    presentation: presentation(!collapsed),
                },
                created_at: Timestamp::from_millis(i64::try_from(index + 3).expect("timestamp")),
            }))
            .expect("collapse commit");
    }
    for index in 0..10_u64 {
        store
            .commit(&OperationBatch::HistoryMove {
                operation_id: ids.operation_id(),
                session_id,
                scope: UndoScope::Board,
                undo: true,
                sequence: OperationSequence::new(506 + index),
                at: Timestamp::from_millis(i64::try_from(600 + index).expect("timestamp")),
            })
            .expect("undo commit");
    }

    store
        .compact_session(session_id)
        .expect("compact redo history");
    let snapshot = store.load_session(session_id).expect("compacted snapshot");
    assert_eq!(
        snapshot.board_operations.len() - snapshot.board_history_cursor,
        10
    );
    assert!(snapshot.board_history_cursor >= 1);
}

const fn presentation(collapsed: bool) -> ThoughtPresentation {
    if collapsed {
        ThoughtPresentation::Collapsed
    } else {
        ThoughtPresentation::Automatic
    }
}

#[test]
fn editor_compaction_keeps_recent_revisions_and_cursor() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-editor-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "0", 2);
    let mut content = "0".to_owned();
    for index in 1..=205_u64 {
        let next = index.to_string();
        let edit = one_effect(
            &mut state,
            Action::EditThought {
                thought_id,
                revision_id: ids.revision_id(),
                before_content: content.clone(),
                after_content: next.clone(),
                before_annotations: Vec::new(),
                after_annotations: Vec::new(),
                before_cursor: TextPosition::new(0, content.len()),
                after_cursor: TextPosition::new(0, next.len()),
                at: Timestamp::from_millis(i64::try_from(index + 2).expect("timestamp")),
            },
        );
        persist_effect(&mut store, &edit);
        content = next;
    }

    store.compact_session(session_id).expect("compact editor");
    let snapshot = store.load_session(session_id).expect("snapshot");
    assert_eq!(snapshot.revisions.len(), 200);
    assert_eq!(snapshot.editor_history_cursors, [(thought_id, 200)]);
    assert_eq!(
        snapshot.board.thought(thought_id).expect("thought").content,
        "205"
    );

    let mut restored = AppState::from_snapshot(snapshot).expect("restore state");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(300),
        },
    );
    persist_effect(&mut store, &undo);
    assert_eq!(
        store
            .load_session(session_id)
            .expect("undone snapshot")
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "204"
    );
}
