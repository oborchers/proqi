use super::*;
use proqi::{
    adapters::{editor::RopeEditorFactory, memory::FakeClock},
    domain::SessionId,
    ports::editor::CursorMovement,
    ui::{BoardApp, UiInput, UiKey},
};

#[test]
fn folded_provenance_and_editor_undo_survive_reopen() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-annotations"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let content = "/private/tmp/screenshot.png".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "screenshot.png".to_owned(),
        },
    };
    let create_effects = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![annotation.clone()],
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("create");
    assert!(
        create_effects
            .iter()
            .any(|effect| matches!(effect, Effect::CheckAttachments(_)))
    );
    let create = create_effects
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable create");
    persist_effect(&mut store, create);
    let edit = one_effect(
        &mut state,
        Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: content.clone(),
            after_content: format!("{content}!"),
            before_annotations: vec![annotation.clone()],
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 1),
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &edit);
    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut store, &undo);
    drop(store);

    let snapshot = fixture.open().load_session(session_id).expect("reopen");
    let restored = snapshot.board.thought(thought_id).expect("thought");
    assert_eq!(restored.content, content);
    assert_eq!(restored.annotations, [annotation]);
    assert_eq!(snapshot.editor_history_cursors, [(thought_id, 0)]);
}

#[test]
fn invocation_reference_projection_survives_protocol_nine_migration() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_100_000);
    let mut state = session_state(&mut ids, &test_path("proqi-invocation-reference"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let content = "Herdr collaborator: coaching-philipp (claude) at workspace Consulting (w4), tab coaching-philipp (w4:t2), pane w4:p2".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::InvocationReference {
            display_name: "@coaching-philipp · claude".to_owned(),
        },
    };
    let effects = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![annotation.clone()],
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("create");
    let create = effects
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable create");
    persist_effect(&mut store, create);
    drop(store);
    Connection::open(&fixture.config.database_path)
        .expect("version eight database")
        .execute_batch(
            "DROP TABLE onboarding_state;
             DELETE FROM migration_history WHERE version IN (9, 10, 11);
             UPDATE schema_meta SET schema_version = 8, storage_protocol = 8;",
        )
        .expect("downgrade protocol stamp");

    let snapshot = fixture.open().load_session(session_id).expect("reopen");
    let restored = snapshot.board.thought(thought_id).expect("thought");
    assert_eq!(restored.content, content);
    assert_eq!(restored.annotations, [annotation]);
    let version = Connection::open(&fixture.config.database_path)
        .expect("migrated database")
        .query_row("SELECT schema_version FROM schema_meta", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("schema version");
    assert_eq!(version, i64::from(SUPPORTED_SCHEMA_VERSION));
}

#[test]
fn protocol_ten_loads_structurally_valid_direct_shortcut_bytes_and_rejects_corruption() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_200_000);
    let mut state = session_state(&mut ids, &test_path("proqi-shortcut-protocol-ten"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let effects = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: "Press Enter".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("plain create");
    persist_effect(&mut store, &effects[0]);
    drop(store);

    let connection = Connection::open(&fixture.config.database_path).expect("direct fixture");
    connection
        .execute(
            "UPDATE thoughts SET annotations_json = ?1 WHERE content = 'Press Enter'",
            [r#"[{"start":6,"end":11,"kind":{"kind":"shortcut_emphasis"}}]"#],
        )
        .expect("same-user direct annotation bytes");
    connection
        .execute_batch(
            "DROP TABLE onboarding_state;
             DELETE FROM migration_history WHERE version IN (10, 11);
             UPDATE schema_meta SET schema_version = 9, storage_protocol = 9;",
        )
        .expect("version nine protocol stamp");
    drop(connection);

    let snapshot = fixture
        .open()
        .load_session(session_id)
        .expect("protocol ten reopen");
    let restored = snapshot.board.thought(thought_id).expect("thought");
    assert_eq!(restored.content, "Press Enter");
    assert_eq!(restored.annotations.len(), 1);
    assert!(restored.annotations[0].is_shortcut_emphasis());

    Connection::open(&fixture.config.database_path)
        .expect("corruption fixture")
        .execute(
            "UPDATE thoughts SET annotations_json = ?1 WHERE content = 'Press Enter'",
            [r#"[{"start":6,"end":11,"kind":{"kind":"future_style"}}]"#],
        )
        .expect("unknown kind bytes");
    assert!(matches!(
        fixture.open().load_session(session_id),
        Err(StoreError::Corrupt(_))
    ));

    Connection::open(&fixture.config.database_path)
        .expect("range corruption fixture")
        .execute(
            "UPDATE thoughts SET annotations_json = ?1 WHERE content = 'Press Enter'",
            [r#"[{"start":6,"end":99,"kind":{"kind":"shortcut_emphasis"}}]"#],
        )
        .expect("out-of-bounds range bytes");
    assert!(matches!(
        fixture.open().load_session(session_id),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn shortcut_edit_undo_and_redo_restore_exact_ranges_across_restarts() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_300_000);
    let mut state = session_state(&mut ids, &test_path("proqi-shortcut-restart-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let create = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: "AA Enter ZZ".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    persist_effect(&mut store, &create);
    drop(store);
    Connection::open(&fixture.config.database_path)
        .expect("direct fixture")
        .execute(
            "UPDATE thoughts SET annotations_json = ?1 WHERE content = 'AA Enter ZZ'",
            [r#"[{"start":3,"end":8,"kind":{"kind":"shortcut_emphasis"}}]"#],
        )
        .expect("same-user direct annotation bytes");

    edit_shortcut_prefix(&fixture, session_id, &mut ids);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("edited reopen");
    let edited = snapshot.board.thought(thought_id).expect("edited thought");
    assert_eq!(edited.content, "!AA Enter ZZ");
    assert_eq!(
        (edited.annotations[0].start, edited.annotations[0].end),
        (4, 9)
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate edited history");
    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut store, &undo);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("undo reopen");
    let undone = snapshot.board.thought(thought_id).expect("undone thought");
    assert_eq!(undone.content, "AA Enter ZZ");
    assert_eq!(
        (undone.annotations[0].start, undone.annotations[0].end),
        (3, 8)
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate undone history");
    let redo = one_effect(
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut store, &redo);
    drop(store);

    let redone = fixture
        .open()
        .load_session(session_id)
        .expect("redo reopen");
    let redone = redone.board.thought(thought_id).expect("redone thought");
    assert_eq!(redone.content, "!AA Enter ZZ");
    assert_eq!(
        (redone.annotations[0].start, redone.annotations[0].end),
        (4, 9)
    );
}

fn edit_shortcut_prefix(
    fixture: &DatabaseFixture,
    session_id: SessionId,
    ids: &mut FakeIdGenerator,
) {
    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("annotated reopen");
    let mut app = BoardApp::new(
        AppState::from_snapshot(snapshot).expect("rehydrate annotated thought"),
        RopeEditorFactory,
    );
    let clock = FakeClock::new(Timestamp::from_millis(3));
    app.handle(UiInput::Key(UiKey::Enter), ids, &clock);
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::DocumentStart,
            extend_selection: false,
        }),
        ids,
        &clock,
    );
    app.handle(UiInput::Key(UiKey::Character('!')), ids, &clock);
    let effects = app.flush_pending_edit(ids, &clock);
    assert_eq!(effects.len(), 1);
    persist_effect(&mut store, &effects[0]);
}

#[test]
fn placeholder_space_revision_undo_and_redo_survive_every_reopen() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_400_000);
    let mut state = session_state(&mut ids, &test_path("proqi-placeholder-space-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let value = "/tmp/restart.png";
    let content = format!("α {value} z");
    let start = "α ".len();
    let annotation = ContentAnnotation {
        start,
        end: start + value.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "restart.png".to_owned(),
        },
    };
    let create = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![annotation.clone()],
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("create");
    let create = create
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable create");
    persist_effect(&mut store, create);
    drop(store);
    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("created reopen");
    let mut app = BoardApp::new(
        AppState::from_snapshot(snapshot).expect("rehydrate created thought"),
        RopeEditorFactory,
    );
    let clock = FakeClock::new(Timestamp::from_millis(3));
    select_restart_placeholder(&mut app, &mut ids, &clock);
    let effects = app.handle(UiInput::Key(UiKey::UnmodifiedSpace), &mut ids, &clock);
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("one placeholder Space revision");
    };
    assert_eq!(revision.after_content, format!("α  {value} z"));
    assert_eq!(revision.after_annotations[0].start, start + 1);
    persist_effect(&mut store, &effects[0]);
    drop(store);
    assert_placeholder_history_restarts(
        &fixture,
        &mut ids,
        session_id,
        thought_id,
        &content,
        &annotation,
        &format!("α  {value} z"),
    );
}

fn select_restart_placeholder(app: &mut BoardApp, ids: &mut FakeIdGenerator, clock: &FakeClock) {
    app.handle(UiInput::Key(UiKey::Enter), ids, clock);
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::DocumentStart,
            extend_selection: false,
        }),
        ids,
        clock,
    );
    for _ in 0..2 {
        app.handle(
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::GraphemeForward,
                extend_selection: false,
            }),
            ids,
            clock,
        );
    }
}

fn assert_placeholder_history_restarts(
    fixture: &DatabaseFixture,
    ids: &mut FakeIdGenerator,
    session_id: proqi::domain::SessionId,
    thought_id: ThoughtId,
    content: &str,
    annotation: &ContentAnnotation,
    moved_content: &str,
) {
    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("edited reopen");
    let edited = snapshot.board.thought(thought_id).expect("edited thought");
    assert_eq!(edited.content, moved_content);
    assert_eq!(edited.annotations[0].start, annotation.start + 1);
    let mut state = AppState::from_snapshot(snapshot).expect("rehydrate edited history");
    let undo = reduce(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(4),
        },
    )
    .expect("undo");
    let undo = undo
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable undo");
    persist_effect(&mut store, undo);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("undo reopen");
    let undone = snapshot.board.thought(thought_id).expect("undone thought");
    assert_eq!(undone.content, content);
    assert_eq!(undone.annotations, vec![annotation.clone()]);
    state = AppState::from_snapshot(snapshot).expect("rehydrate undone history");
    let redo = reduce(
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(5),
        },
    )
    .expect("redo");
    let redo = redo
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable redo");
    persist_effect(&mut store, redo);
    drop(store);

    let redone = fixture
        .open()
        .load_session(session_id)
        .expect("redo reopen");
    let redone = redone.board.thought(thought_id).expect("redone thought");
    assert_eq!(redone.content, moved_content);
    assert_eq!(redone.annotations[0].start, annotation.start + 1);
}
