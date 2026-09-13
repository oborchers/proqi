use super::*;

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

    let undo = app.handle(
        UiInput::Key(crate::ui::UiKey::Undo),
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(4)),
    );
    assert!(matches!(
        undo.as_slice(),
        [Effect::CommitHistoryMove { undo: true, .. }]
    ));
    assert_eq!(app.state.mode, InteractionMode::Compose);
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
