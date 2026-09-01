use super::*;

fn seeded_history(
    fixture: &DatabaseFixture,
) -> (SqliteStore, AppState, FakeIdGenerator, ThoughtId, ThoughtId) {
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-history"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "second", 3);
    for action in [
        Action::MoveThought {
            operation_id: ids.operation_id(),
            thought_id: second,
            to: 0,
            at: Timestamp::from_millis(4),
        },
        Action::EditThought {
            thought_id: first,
            revision_id: ids.revision_id(),
            before_content: "first".to_owned(),
            after_content: "first edited\r\n日本語".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 5),
            after_cursor: TextPosition::new(1, 3),
            at: Timestamp::from_millis(5),
        },
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id: first },
            at: Timestamp::from_millis(6),
        },
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(7),
        },
    ] {
        let effect = one_effect(&mut state, action);
        persist_effect(&mut store, &effect);
    }
    (store, state, ids, first, second)
}

fn verify_trash_restore_and_prune(store: &mut SqliteStore, session_id: proqi::domain::SessionId) {
    store
        .trash_session(session_id, Timestamp::from_millis(10))
        .expect("trash");
    assert!(
        store
            .search_sessions(&SessionQuery::default())
            .expect("live search")
            .iter()
            .all(|hit| hit.id != session_id)
    );
    assert!(
        store
            .search_sessions(&SessionQuery {
                include_trashed: true,
                ..SessionQuery::default()
            })
            .expect("trash search")
            .iter()
            .any(|hit| hit.id == session_id && hit.trashed)
    );
    store.restore_session(session_id).expect("restore");
    assert!(
        store
            .load_session(session_id)
            .expect("restored")
            .board
            .session
            .deleted_at
            .is_none()
    );
    assert!(matches!(
        store.prune_session(session_id),
        Err(StoreError::Conflict(_))
    ));
    store
        .trash_session(session_id, Timestamp::from_millis(11))
        .expect("trash again");
    store.prune_session(session_id).expect("prune");
    assert!(matches!(
        store.load_session(session_id),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn opens_with_durable_pragmas_and_round_trips_session() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    assert_eq!(store.journal_mode().expect("journal"), "wal");
    assert_eq!(store.synchronous_level().expect("synchronous"), 2);
    store.quick_check().expect("integrity");

    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let state = session_state(&mut ids, &test_path("proqi-sqlite"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let snapshot = store
        .load_session(state.board.session.id)
        .expect("load session");
    assert_eq!(snapshot.board.session, state.board.session);
    assert!(snapshot.board.thoughts().is_empty());
}

#[test]
fn explicitly_created_blank_thought_survives_reopen_and_remains_undoable() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-blank"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let blank = create_thought(&mut store, &mut state, &mut ids, "", 2);
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen blank");
    assert_eq!(snapshot.board.thought(blank).expect("blank").content, "");
    let mut restored = AppState::from_snapshot(snapshot).expect("rehydrate blank");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut reopened, &undo);
    assert!(
        reopened
            .load_session(session_id)
            .expect("undone blank")
            .board
            .live_thoughts()
            .is_empty()
    );
}

#[test]
fn expanded_presentation_survives_reopen_and_board_undo() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-presentation"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "long thought", 2);
    let presentation = one_effect(
        &mut state,
        Action::SetPresentation {
            operation_id: ids.operation_id(),
            thought_id,
            presentation: ThoughtPresentation::Expanded,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &presentation);
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened
        .load_session(session_id)
        .expect("reopen presentation");
    assert_eq!(
        snapshot
            .board
            .thought(thought_id)
            .expect("thought")
            .presentation,
        ThoughtPresentation::Expanded
    );
    let mut restored = AppState::from_snapshot(snapshot).expect("rehydrate presentation");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut reopened, &undo);
    assert_eq!(
        reopened
            .load_session(session_id)
            .expect("undo presentation")
            .board
            .thought(thought_id)
            .expect("thought")
            .presentation,
        ThoughtPresentation::Automatic
    );
}

#[test]
fn board_editor_and_persistent_history_survive_reopen() {
    let fixture = DatabaseFixture::new();
    let (store, mut state, mut ids, first, second) = seeded_history(&fixture);
    let session_id = state.board.session.id;
    drop(store);

    let mut reopened = fixture.open();
    let snapshot = reopened.load_session(session_id).expect("reopen snapshot");
    let live = snapshot.board.live_thoughts();
    assert_eq!(
        live.iter().map(|thought| thought.id).collect::<Vec<_>>(),
        vec![first, second]
    );
    assert_eq!(
        snapshot.board.thought(first).expect("first").content,
        "first"
    );
    assert_eq!(snapshot.board_operations.len(), 3);
    assert_eq!(snapshot.board_history_cursor, 2);
    assert_eq!(snapshot.revisions.len(), 1);
    assert_eq!(
        snapshot
            .editor_history_cursors
            .iter()
            .find(|(thought, _)| *thought == first)
            .map(|(_, cursor)| *cursor),
        Some(0)
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate application state");

    let redo_board = one_effect(
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(8),
        },
    );
    persist_effect(&mut reopened, &redo_board);
    let redo_editor = one_effect(
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id: first },
            at: Timestamp::from_millis(9),
        },
    );
    persist_effect(&mut reopened, &redo_editor);
    let redone = reopened.load_session(session_id).expect("redone");
    assert_eq!(redone.board.live_thoughts()[0].id, second);
    assert_eq!(
        redone.board.thought(first).expect("first").content,
        "first edited\r\n日本語"
    );
}

#[test]
fn commits_are_idempotent_but_identity_reuse_is_rejected() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-idempotency"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = ids.thought_id();
    let operation_id = ids.operation_id();
    let effect = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id,
            content: "same request".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    let first = persist_effect(&mut store, &effect);
    let replay = persist_effect(&mut store, &effect);
    assert!(!first.idempotent_replay);
    assert!(replay.idempotent_replay);
    assert_eq!(replay.identity, DurableIdentity::Operation(operation_id));

    let Effect::CommitBoardOperation(mut changed) = effect else {
        panic!("board operation")
    };
    changed.created_at = Timestamp::from_millis(99);
    assert!(matches!(
        store.commit(&OperationBatch::Board(changed)),
        Err(StoreError::Conflict(_))
    ));
    assert_eq!(
        store
            .load_session(state.board.session.id)
            .expect("snapshot")
            .board
            .live_thoughts()
            .len(),
        1
    );
}

#[test]
fn search_index_is_rebuildable_and_trash_is_recoverable() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);

    let current_project = test_path("current-project");
    let other_project = test_path("other-project");
    let mut first = session_state(&mut ids, &current_project);
    first
        .board
        .session
        .rename(Some("August research".to_owned()))
        .expect("rename");
    store
        .commit(&OperationBatch::CreateSession(first.board.session.clone()))
        .expect("first session");
    create_thought(&mut store, &mut first, &mut ids, "  \n", 2);
    create_thought(
        &mut store,
        &mut first,
        &mut ids,
        "Summarize Cloud and Codex identity changes",
        3,
    );
    create_thought(
        &mut store,
        &mut first,
        &mut ids,
        "Third thought with a browser-only needle",
        4,
    );

    let mut second = session_state(&mut ids, &other_project);
    store
        .commit(&OperationBatch::CreateSession(second.board.session.clone()))
        .expect("second session");
    create_thought(&mut store, &mut second, &mut ids, "Unrelated prompt", 3);

    let hits = store
        .search_sessions(&SessionQuery {
            text: Some("Cloud identity".to_owned()),
            include_trashed: false,
            current_directory: Some(current_project.clone()),
        })
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, first.board.session.id);
    assert_eq!(hits[0].thought_count, 3);
    assert_eq!(hits[0].origin_cwd, current_project);
    assert_eq!(hits[0].last_opened_cwd, test_path("current-project"));
    assert_eq!(
        hits[0].previews,
        [
            "Summarize Cloud and Codex identity changes",
            "Third thought with a browser-only needle"
        ]
    );
    assert_eq!(
        hits[0].excerpt,
        "Summarize Cloud and Codex identity changes"
    );
    assert!(hits[0].search_content.contains("browser-only needle"));
    let ranked = store
        .search_sessions(&SessionQuery {
            current_directory: Some(other_project),
            ..SessionQuery::default()
        })
        .expect("ranked sessions");
    assert_eq!(ranked[0].id, second.board.session.id);
    store.rebuild_search_index().expect("rebuild FTS");
    assert_eq!(
        store
            .search_sessions(&SessionQuery {
                text: Some("August".to_owned()),
                ..SessionQuery::default()
            })
            .expect("name search")[0]
            .id,
        first.board.session.id
    );

    verify_trash_restore_and_prune(&mut store, first.board.session.id);
}

#[test]
fn integration_context_round_trips_without_conversation_content() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let state = session_state(&mut ids, &test_path("proqi-context"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let context = IntegrationContext {
        provider: "herdr".to_owned(),
        direction: Direction::Left,
        agent_kind: proqi::ports::agent::CODEX_AGENT_KIND.to_owned(),
        agent_name: "Codex".to_owned(),
        workspace_hint: Some("workspace".to_owned()),
        tab_hint: Some("tab".to_owned()),
        pane_hint: Some("pane".to_owned()),
        verified_at: Timestamp::from_millis(2),
    };
    store
        .commit(&OperationBatch::IntegrationContext {
            session_id,
            context: Some(context.clone()),
        })
        .expect("context");
    assert_eq!(
        store
            .load_session(session_id)
            .expect("snapshot")
            .integration_context,
        Some(context)
    );
}
