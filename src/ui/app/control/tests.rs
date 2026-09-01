use crate::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, ApplicationError, InteractionMode},
    domain::{ContentAnnotation, Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{control::ControlMutation, editor::EditCommand, environment::IdGenerator},
    ui::UiInput,
};

use super::BoardApp;

#[test]
fn generic_control_add_cannot_author_shortcut_emphasis_but_preservation_can_retain_it() {
    let mut ids = FakeIdGenerator::new(1_725_190_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-shortcut-authority"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let annotation = ContentAnnotation::shortcut(6, 11);
    let thought_id = ids.thought_id();
    let add = ControlMutation::Add {
        operation_id: ids.operation_id(),
        thought_id,
        content: "Press Enter".to_owned(),
        annotations: vec![annotation.clone()],
        position: None,
    };

    assert_eq!(
        app.handle_control(&add, &FakeClock::new(Timestamp::from_millis(2))),
        Err(ApplicationError::InvalidState)
    );
    assert!(app.state.board.thought(thought_id).is_none());

    let preserved_id = ids.thought_id();
    app.handle_control(
        &ControlMutation::PreserveAdd {
            operation_id: ids.operation_id(),
            thought_id: preserved_id,
            content: "Press Enter".to_owned(),
            annotations: vec![annotation.clone()],
            position: None,
        },
        &FakeClock::new(Timestamp::from_millis(3)),
    )
    .expect("purpose-specific preservation");
    assert_eq!(
        app.state
            .board
            .thought(preserved_id)
            .expect("preserved thought")
            .annotations,
        [annotation]
    );
}

#[test]
fn active_add_preserves_the_users_live_editor_and_focus() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-focus"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let original_id = ids.thought_id();
    let original = Thought::new(
        original_id,
        session.id,
        "editing".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![original]).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );
    app.state.mode = InteractionMode::Edit {
        thought_id: original_id,
    };
    app.sync_editor_from_state();
    app.apply_edit(EditCommand::InsertChar('!'));
    let editor_before = app.editor_snapshot().expect("live editor draft");
    assert!(app.has_pending_edit());
    let added_id = ids.thought_id();
    let mutation = ControlMutation::Add {
        operation_id: ids.operation_id(),
        thought_id: added_id,
        content: "external".to_owned(),
        annotations: Vec::new(),
        position: None,
    };

    let effects = app
        .handle_control(&mutation, &FakeClock::new(Timestamp::from_millis(2)))
        .expect("control add");

    assert_eq!(effects.len(), 1);
    assert_eq!(app.editor_snapshot(), Some(editor_before));
    assert!(app.has_pending_edit());
    assert_eq!(app.state.focused_thought, Some(original_id));
    assert_eq!(
        app.state.mode,
        InteractionMode::Edit {
            thought_id: original_id
        }
    );
    assert_eq!(
        app.state
            .board
            .thought(added_id)
            .expect("added thought")
            .content,
        "external"
    );
}

#[test]
fn active_add_preserves_compose_editor_and_queued_typeahead() {
    let mut ids = FakeIdGenerator::new(1_725_210_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-compose"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );
    let added_id = ids.thought_id();

    let effects = app
        .handle_control(
            &ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: added_id,
                content: "external".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(2)),
        )
        .expect("control add");

    assert_eq!(effects.len(), 1);
    assert_eq!(app.state.mode, InteractionMode::Compose);
    assert_eq!(app.editor_snapshot().expect("compose editor").content, "");
    let typing = app.handle(
        UiInput::Key(crate::ui::UiKey::Character('n')),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(3)),
    );
    assert!(matches!(
        typing.as_slice(),
        [crate::application::Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        app.state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["external", "n"]
    );
}

#[test]
fn ui_paste_and_forwarded_add_produce_the_same_state_and_durable_effect() {
    let mut session_ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        session_ids.session_id(),
        std::env::temp_dir().join("proqi-entry-point-conformance"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    let state = AppState::new(board);
    let mut ui = BoardApp::new(state.clone(), crate::adapters::editor::RopeEditorFactory);
    let mut forwarded = BoardApp::new(state, crate::adapters::editor::RopeEditorFactory);
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let mut ui_ids = FakeIdGenerator::new(1_725_300_000_000);
    let mut forwarded_ids = FakeIdGenerator::new(1_725_300_000_000);
    let thought_id = forwarded_ids.thought_id();
    let operation_id = forwarded_ids.operation_id();

    let ui_effects = ui.handle(
        UiInput::Paste("same content".to_owned()),
        &mut ui_ids,
        &clock,
    );
    let forwarded_effects = forwarded
        .handle_control(
            &ControlMutation::Add {
                operation_id,
                thought_id,
                content: "same content".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &clock,
        )
        .expect("forwarded add");

    assert_eq!(forwarded.state.board, ui.state.board);
    assert_eq!(forwarded.state.durability, ui.state.durability);
    assert_eq!(forwarded.state.mode, InteractionMode::Compose);
    assert!(matches!(ui.state.mode, InteractionMode::Edit { .. }));
    assert_eq!(forwarded_effects, ui_effects);
    assert_eq!(forwarded_effects.len(), 1);
    assert!(forwarded_effects[0].persistence_batch().is_some());
}

#[test]
fn external_editor_undo_refreshes_content_and_restores_cursor() {
    use crate::{domain::TextPosition, ports::editor::EditCommand};

    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-editor"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session.id,
        "base".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );
    app.state.mode = InteractionMode::Edit { thought_id };
    app.sync_editor_from_state();
    app.apply_edit(EditCommand::Paste(" changed".to_owned()));
    let _effects = app.flush_pending_edit(&mut ids, &FakeClock::new(Timestamp::from_millis(2)));
    let mutation = ControlMutation::History {
        operation_id: ids.operation_id(),
        scope: crate::domain::UndoScope::Editor { thought_id },
        undo: true,
    };

    app.handle_control(&mutation, &FakeClock::new(Timestamp::from_millis(3)))
        .expect("external undo");

    let snapshot = app.editor_snapshot().expect("editor");
    assert_eq!(snapshot.content, "base");
    assert_eq!(snapshot.cursor, TextPosition::new(0, 4));
}

#[test]
fn external_final_deletion_does_not_force_compose() {
    let mut ids = FakeIdGenerator::new(1_725_220_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-empty"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session.id,
        "remote source".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );

    app.handle_control(
        &ControlMutation::Delete {
            operation_id: ids.operation_id(),
            thought_id,
        },
        &FakeClock::new(Timestamp::from_millis(2)),
    )
    .expect("control delete");

    assert!(app.state.board.live_thoughts().is_empty());
    assert_eq!(app.state.mode, InteractionMode::Board);
    assert!(app.editor_snapshot().is_none());
}

#[test]
fn external_replace_is_an_editor_revision_that_undoes_normally() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-replace"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session.id,
        "before".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );
    let clock = FakeClock::new(Timestamp::from_millis(2));

    let effects = app
        .handle_control(
            &ControlMutation::Replace {
                revision_id: ids.revision_id(),
                thought_id,
                expected_digest: None,
                content: "after".to_owned(),
            },
            &clock,
        )
        .expect("replace");
    assert!(matches!(
        effects.as_slice(),
        [crate::application::Effect::CommitRevision(_)]
    ));
    assert_eq!(
        app.state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "after"
    );

    app.handle_control(
        &ControlMutation::History {
            operation_id: ids.operation_id(),
            scope: crate::domain::UndoScope::Editor { thought_id },
            undo: true,
        },
        &clock,
    )
    .expect("undo replacement");
    assert_eq!(
        app.state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "before"
    );
}

#[test]
fn thought_mutations_are_locked_from_submission_intent_until_completion() {
    use crate::{
        application::ApplicationError,
        domain::Direction,
        ports::agent::{
            AgentDeliveryCapabilities, AgentSessionBinding, AgentState, AgentTarget,
            CODEX_AGENT_KIND, HarnessKind, PaneContext, PaneRect,
        },
        ui::UiKey,
    };

    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-lock"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session.id,
        "locked".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, vec![thought]).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let source = PaneContext {
        workspace_id: "workspace".to_owned(),
        tab_id: "tab".to_owned(),
        pane_id: "source".to_owned(),
        rect: PaneRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
    };
    app.complete_agent_discovery(Ok(vec![AgentTarget {
        provider: "herdr".to_owned(),
        protocol: 1,
        direction: Direction::Left,
        pane_id: "target".to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: HarnessKind::new(CODEX_AGENT_KIND).expect("fixture harness"),
        agent_name: "Codex".to_owned(),
        agent_session: AgentSessionBinding::established("agent-session").expect("fixture session"),
        readiness: AgentState::Working,
        delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: PaneRect {
            x: 10,
            y: 0,
            width: 10,
            height: 10,
        },
        source,
    }]));
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let effects = app.handle(UiInput::Key(UiKey::Character('s')), &mut ids, &clock);
    assert!(matches!(
        effects.as_slice(),
        [crate::application::Effect::PrepareSubmission(_)]
    ));

    let error = app
        .handle_control(
            &ControlMutation::Delete {
                operation_id: ids.operation_id(),
                thought_id,
            },
            &clock,
        )
        .expect_err("locked thought");
    assert_eq!(error, ApplicationError::ThoughtLocked(thought_id));
}
