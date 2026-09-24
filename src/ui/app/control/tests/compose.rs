use super::*;
use crate::{ports::editor::CursorMovement, ui::UiKey};

#[test]
fn first_active_owner_add_hands_empty_compose_to_board_focus() {
    let mut ids = FakeIdGenerator::new(1_725_209_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-first-control-focus"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );
    assert_eq!(app.state.mode, InteractionMode::Compose);
    let added_id = ids.thought_id();
    let effects = app
        .handle_control(
            &ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: added_id,
                content: "first API thought".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(2)),
        )
        .expect("first active owner add");
    let sequence = effects[0]
        .persistence_batch()
        .expect("commit batch")
        .sequence()
        .expect("sequence");
    app.acknowledge_persistence(sequence, true);

    assert_eq!(app.state.mode, InteractionMode::Board);
    assert_eq!(app.state.focused_thought_id(), Some(added_id));
    assert!(!app.thought_selected(added_id));
    assert!(app.editor_snapshot().is_none());
    let history = app.state.board_history().len();
    let sequence = app.state.board.session.last_durable_sequence;
    for key in [
        UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        },
        UiKey::Move {
            movement: CursorMovement::VisualUp,
            extend_selection: false,
        },
        UiKey::Character('j'),
        UiKey::Character('k'),
    ] {
        assert!(
            app.handle(
                UiInput::Key(key),
                &mut ids,
                &FakeClock::new(Timestamp::from_millis(3))
            )
            .is_empty()
        );
    }
    assert_eq!(app.state.focused_thought_id(), Some(added_id));
    assert!(!app.thought_selected(added_id));
    assert!(app.editor_snapshot().is_none());
    assert_eq!(app.state.board_history().len(), history);
    assert_eq!(app.state.board.session.last_durable_sequence, sequence);
}

#[test]
fn failed_first_add_waits_for_retry_and_later_add_keeps_focus() {
    let mut ids = FakeIdGenerator::new(1_725_209_100_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let first = ids.thought_id();
    let effects = app
        .handle_control(
            &ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: first,
                content: "first".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(2)),
        )
        .expect("first add");
    let sequence = effects[0]
        .persistence_batch()
        .expect("batch")
        .sequence()
        .expect("sequence");
    app.acknowledge_persistence(sequence, false);
    assert_eq!(app.state.mode, InteractionMode::Compose);
    app.acknowledge_persistence(sequence, true);
    assert_eq!(app.state.focused_thought_id(), Some(first));
    let second = ids.thought_id();
    let effects = app
        .handle_control(
            &ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: second,
                content: "second".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(3)),
        )
        .expect("later add");
    let sequence = effects[0]
        .persistence_batch()
        .expect("batch")
        .sequence()
        .expect("sequence");
    app.acknowledge_persistence(sequence, true);
    assert_eq!(app.state.focused_thought_id(), Some(first));
    assert_eq!(app.state.mode, InteractionMode::Board);
}

#[test]
fn first_separator_and_pending_compose_clipboard_keep_their_owners() {
    let mut ids = FakeIdGenerator::new(1_725_209_200_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let request = app.handle(
        UiInput::Key(UiKey::PasteClipboard),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(2)),
    );
    let [Effect::ReadClipboard { request_id }] = request.as_slice() else {
        panic!("Compose clipboard request");
    };
    let request_id = *request_id;
    let operation_id = ids.operation_id();
    let separator_id =
        crate::domain::SeparatorId::from_database_bytes(operation_id.database_bytes())
            .expect("paired separator ID");
    let effects = app
        .handle_control(
            &ControlMutation::InsertSeparator {
                operation_id,
                separator_id,
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(3)),
        )
        .expect("first separator");
    let sequence = effects[0]
        .persistence_batch()
        .expect("batch")
        .sequence()
        .expect("sequence");
    app.acknowledge_persistence(sequence, true);
    assert_eq!(app.state.mode, InteractionMode::Compose);
    let completion = app.complete_clipboard_read(
        request_id,
        Ok(String::new()),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(4)),
    );
    assert!(completion.is_empty());
    assert_eq!(app.state.mode, InteractionMode::Board);
    assert_eq!(app.state.focused_item, Some(separator_id.into()));
    assert!(app.editor_snapshot().is_none());
}

#[test]
fn accepted_compose_clipboard_result_materializes_after_external_save() {
    let mut ids = FakeIdGenerator::new(1_725_209_300_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let request = app.handle(
        UiInput::Key(UiKey::PasteClipboard),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(2)),
    );
    let [Effect::ReadClipboard { request_id }] = request.as_slice() else {
        panic!("Compose read request");
    };
    let request_id = *request_id;
    let first = ids.thought_id();
    let effects = app
        .handle_control(
            &ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: first,
                content: "external".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(3)),
        )
        .expect("first external add");
    let sequence = effects[0]
        .persistence_batch()
        .expect("batch")
        .sequence()
        .expect("sequence");
    app.acknowledge_persistence(sequence, true);
    assert_eq!(app.state.mode, InteractionMode::Compose);
    let effects = app.complete_clipboard_read(
        request_id,
        Ok("owned clipboard".to_owned()),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(4)),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(live_content(&app), ["external", "owned clipboard"]);
    assert!(matches!(app.state.mode, InteractionMode::Edit { .. }));
    assert_ne!(app.state.focused_thought_id(), Some(first));
}

#[test]
fn pending_compose_clipboard_does_not_lose_first_focus_after_api_prepend() {
    let mut ids = FakeIdGenerator::new(1_725_209_350_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let request = app.handle(
        UiInput::Key(UiKey::PasteClipboard),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(2)),
    );
    let [Effect::ReadClipboard { request_id }] = request.as_slice() else {
        panic!("Compose read request");
    };
    let request_id = *request_id;
    let first = ids.thought_id();
    for (thought_id, content, position, at) in [
        (first, "first", None, 3),
        (ids.thought_id(), "prepended", Some(0), 4),
    ] {
        let effects = app
            .handle_control(
                &ControlMutation::Add {
                    operation_id: ids.operation_id(),
                    thought_id,
                    content: content.to_owned(),
                    annotations: Vec::new(),
                    position,
                },
                &FakeClock::new(Timestamp::from_millis(at)),
            )
            .expect("API addition");
        let sequence = effects[0]
            .persistence_batch()
            .expect("batch")
            .sequence()
            .expect("sequence");
        app.acknowledge_persistence(sequence, true);
    }
    assert_eq!(app.state.mode, InteractionMode::Compose);
    assert_eq!(live_content(&app), ["prepended", "first"]);
    assert!(
        app.complete_clipboard_read(
            request_id,
            Ok(String::new()),
            &mut ids,
            &FakeClock::new(Timestamp::from_millis(5)),
        )
        .is_empty()
    );
    assert_eq!(app.state.mode, InteractionMode::Board);
    assert_eq!(app.state.focused_thought_id(), Some(first));
    assert!(app.editor_snapshot().is_none());
}

#[test]
fn rolled_back_first_control_effect_cannot_trigger_a_late_handoff() {
    let mut ids = FakeIdGenerator::new(1_725_209_400_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let before = app.state.clone();
    let effects = app
        .handle_control(
            &ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: ids.thought_id(),
                content: "rolled back".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
            &FakeClock::new(Timestamp::from_millis(2)),
        )
        .expect("optimistic add");
    let sequence = effects[0]
        .persistence_batch()
        .expect("batch")
        .sequence()
        .expect("sequence");
    app.restore_control_state(before);
    app.acknowledge_persistence(sequence, true);
    assert_eq!(app.state.mode, InteractionMode::Compose);
    assert!(app.state.board.live_items().is_empty());
    assert!(app.state.focused_item.is_none());
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
    assert_eq!(live_content(&app), ["external", "n"]);
    let first_sequence = effects[0]
        .persistence_batch()
        .expect("batch")
        .sequence()
        .expect("sequence");
    app.acknowledge_persistence(first_sequence, true);
    assert_eq!(
        app.state.mode,
        InteractionMode::Edit {
            thought_id: app.state.focused_thought_id().expect("typed thought")
        }
    );

    let undo = app.handle(
        UiInput::Key(crate::ui::UiKey::Undo),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(4)),
    );
    assert!(matches!(
        undo.as_slice(),
        [Effect::CommitHistoryMove { undo: true, .. }]
    ));
    assert_eq!(app.state.mode, InteractionMode::Board);
    assert_eq!(app.state.focused_thought_id(), Some(added_id));
    assert_eq!(live_content(&app), ["external"]);

    let redo = app.handle(
        UiInput::Key(crate::ui::UiKey::Redo),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(5)),
    );
    assert!(matches!(
        redo.as_slice(),
        [Effect::CommitHistoryMove { undo: false, .. }]
    ));
    assert_eq!(
        app.editor_snapshot()
            .expect("restored Compose handoff")
            .content,
        "n"
    );
    assert_eq!(live_content(&app), ["external", "n"]);
}

fn live_content(app: &BoardApp) -> Vec<&str> {
    app.state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect()
}
