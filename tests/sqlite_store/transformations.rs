use super::*;

fn persist_action(store: &mut SqliteStore, state: &mut AppState, action: Action) {
    let effect = one_effect(state, action);
    persist_effect(store, &effect);
}

fn persist_board_history(
    store: &mut SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    undo: bool,
    at: i64,
) {
    let action = if undo {
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(at),
        }
    } else {
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(at),
        }
    };
    persist_action(store, state, action);
}

fn search_matches(
    store: &mut SqliteStore,
    session_id: proqi::domain::SessionId,
    text: &str,
) -> bool {
    store
        .search_sessions(&SessionQuery {
            text: Some(text.to_owned()),
            ..SessionQuery::default()
        })
        .expect("search sessions")
        .iter()
        .any(|hit| hit.id == session_id)
}

fn live_contents(snapshot: &proqi::ports::store::SessionSnapshot) -> Vec<&str> {
    snapshot
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect()
}

fn committed_split() -> (
    DatabaseFixture,
    FakeIdGenerator,
    proqi::domain::SessionId,
    ThoughtId,
    ThoughtId,
) {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-split-restart"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let source = create_thought(&mut store, &mut state, &mut ids, "alpha βeta", 2);
    persist_action(
        &mut store,
        &mut state,
        Action::EditThought {
            thought_id: source,
            revision_id: ids.revision_id(),
            before_content: "alpha βeta".to_owned(),
            after_content: "alpha βeta!".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 10),
            after_cursor: TextPosition::new(0, 11),
            at: Timestamp::from_millis(3),
        },
    );
    persist_action(
        &mut store,
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id: source },
            at: Timestamp::from_millis(4),
        },
    );
    let right = ids.thought_id();
    persist_action(
        &mut store,
        &mut state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: right,
            operation_id: ids.operation_id(),
            expected_content: "alpha βeta".to_owned(),
            expected_annotations: Vec::new(),
            at_byte: 6,
            at: Timestamp::from_millis(5),
        },
    );
    drop(store);
    (fixture, ids, session_id, source, right)
}

#[test]
fn split_is_one_restart_safe_history_unit_with_consistent_search_and_redo() {
    let (fixture, mut ids, session_id, source, right) = committed_split();
    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen split");
    assert_eq!(
        snapshot.board_operations.last().expect("operation").kind,
        BoardOperationKind::Split
    );
    assert_eq!(
        snapshot.board.thought(source).expect("left").content,
        "alpha "
    );
    assert_eq!(
        snapshot.board.thought(right).expect("right").content,
        "βeta"
    );
    assert!(
        snapshot.revisions.is_empty(),
        "split truncates the source redo branch atomically"
    );
    assert!(
        reopened
            .search_sessions(&SessionQuery {
                text: Some("βeta".to_owned()),
                ..SessionQuery::default()
            })
            .expect("search split")
            .iter()
            .any(|hit| hit.id == session_id)
    );
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate split");
    persist_action(
        &mut reopened,
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(6),
        },
    );
    drop(reopened);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen undo");
    assert_eq!(
        snapshot.board.thought(source).expect("source").content,
        "alpha βeta"
    );
    assert!(
        !snapshot
            .board
            .thought(right)
            .expect("retained right")
            .is_live()
    );
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate undo");
    persist_action(
        &mut reopened,
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(7),
        },
    );
    let redone = reopened.load_session(session_id).expect("reopen redo");
    assert_eq!(redone.board.live_thoughts().len(), 2);
    assert_eq!(redone.board.thought(right).expect("right").content, "βeta");
}

#[test]
fn merge_failure_does_not_apply_an_earlier_mutation_or_advance_sequence() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-transform-atomicity"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let source = create_thought(&mut store, &mut state, &mut ids, "unchanged", 2);
    let missing = ids.thought_id();
    let operation = BoardOperation {
        id: ids.operation_id(),
        session_id,
        sequence: OperationSequence::new(2),
        kind: BoardOperationKind::Merge,
        forward: BoardMutation::Batch {
            mutations: vec![
                BoardMutation::ReplaceContent {
                    thought_id: source,
                    before_content: "unchanged".to_owned(),
                    before_annotations: Vec::new(),
                    after_content: "partially changed".to_owned(),
                    after_annotations: Vec::new(),
                },
                BoardMutation::SetDeletion {
                    thought_id: missing,
                    deleted_at: Some(Timestamp::from_millis(3)),
                    position: ThoughtPosition::new(1),
                },
            ],
        },
        inverse: BoardMutation::Batch {
            mutations: Vec::new(),
        },
        created_at: Timestamp::from_millis(3),
    };
    assert!(store.commit(&OperationBatch::Board(operation)).is_err());
    let snapshot = store.load_session(session_id).expect("unchanged snapshot");
    assert_eq!(
        snapshot.board.thought(source).expect("source").content,
        "unchanged"
    );
    assert_eq!(
        snapshot.board.session.last_durable_sequence,
        OperationSequence::new(1)
    );
    assert_eq!(snapshot.board_operations.len(), 1);
}

#[test]
fn extract_undo_and_redo_each_survive_restart_with_consistent_search() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-extract-restart"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let source = create_thought(&mut store, &mut state, &mut ids, "prefix世界suffix", 2);
    let extracted = ids.thought_id();
    persist_action(
        &mut store,
        &mut state,
        Action::ExtractThought {
            thought_id: source,
            new_thought_id: extracted,
            operation_id: ids.operation_id(),
            expected_content: "prefix世界suffix".to_owned(),
            expected_annotations: Vec::new(),
            range: 6..12,
            at: Timestamp::from_millis(3),
        },
    );
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen extract");
    assert_eq!(
        snapshot.board.thought(source).expect("source").content,
        "prefixsuffix"
    );
    assert_eq!(
        snapshot
            .board
            .thought(extracted)
            .expect("extracted")
            .content,
        "世界"
    );
    assert_eq!(
        snapshot.board_operations.last().expect("operation").kind,
        BoardOperationKind::Extract
    );
    assert!(search_matches(&mut store, session_id, "世界"));
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate extract");
    persist_board_history(&mut store, &mut state, &mut ids, true, 4);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen extract undo");
    assert_eq!(
        snapshot.board.thought(source).expect("source").content,
        "prefix世界suffix"
    );
    assert!(
        !snapshot
            .board
            .thought(extracted)
            .expect("retained extraction")
            .is_live()
    );
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate extract undo");
    persist_board_history(&mut store, &mut state, &mut ids, false, 5);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen extract redo");
    assert_eq!(
        snapshot.board.thought(source).expect("source").content,
        "prefixsuffix"
    );
    assert_eq!(
        snapshot
            .board
            .thought(extracted)
            .expect("extracted")
            .content,
        "世界"
    );
}

#[test]
fn merge_undo_and_redo_each_survive_restart_with_recoverable_sources() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-merge-restart"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "one", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "two", 3);
    let third = create_thought(&mut store, &mut state, &mut ids, "三", 4);
    let expected_sources = [first, second, third]
        .into_iter()
        .map(|id| state.board.thought(id).expect("source").clone())
        .collect();
    persist_action(
        &mut store,
        &mut state,
        Action::MergeThoughts {
            operation_id: ids.operation_id(),
            thought_ids: vec![first, second, third],
            expected_sources,
            separator: "\n\n".to_owned(),
            at: Timestamp::from_millis(5),
        },
    );
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen merge");
    assert_eq!(snapshot.board.live_thoughts().len(), 1);
    assert_eq!(
        snapshot.board.thought(first).expect("survivor").content,
        "one\n\ntwo\n\n三"
    );
    assert_eq!(
        snapshot.board_operations.last().expect("operation").kind,
        BoardOperationKind::Merge
    );
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate merge");
    persist_board_history(&mut store, &mut state, &mut ids, true, 6);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen merge undo");
    assert_eq!(live_contents(&snapshot), vec!["one", "two", "三"]);
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate merge undo");
    persist_board_history(&mut store, &mut state, &mut ids, false, 7);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("reopen merge redo");
    assert_eq!(snapshot.board.live_thoughts().len(), 1);
    assert_eq!(
        snapshot.board.thought(first).expect("survivor").content,
        "one\n\ntwo\n\n三"
    );
    assert!(!snapshot.board.thought(second).expect("second").is_live());
    assert!(!snapshot.board.thought(third).expect("third").is_live());
}
