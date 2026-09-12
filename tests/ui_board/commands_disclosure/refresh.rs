use super::*;

#[test]
fn asynchronous_submission_refresh_preserves_the_selected_typed_command() {
    let mut gained = Fixture::new();
    saved_thought(&mut gained, "discovery gain");
    open(&mut gained);
    let edit = gained
        .app
        .palette_view()
        .expect("Commands before discovery")
        .1
        .iter()
        .position(|row| row == "Edit thought")
        .expect("Edit command");
    move_down(&mut gained, edit);
    gained
        .app
        .complete_agent_discovery(Ok(vec![crate::agent::target(
            proqi::domain::Direction::Right,
            "w1:p2",
        )]));
    let (_, rows, selected) = gained.app.palette_view().expect("Commands after discovery");
    assert_eq!(rows[selected], "Edit thought");
    gained.input(crate::key_input(UiKey::Enter));
    assert!(gained.app.editor_snapshot().is_some());

    let mut lost = Fixture::new();
    saved_thought(&mut lost, "discovery loss");
    lost.app
        .complete_agent_discovery(Ok(vec![crate::agent::target(
            proqi::domain::Direction::Right,
            "w1:p2",
        )]));
    open(&mut lost);
    let copy = lost
        .app
        .palette_view()
        .expect("Commands before target loss")
        .1
        .iter()
        .position(|row| row == "Copy thought")
        .expect("Copy command");
    move_down(&mut lost, copy);
    lost.app.complete_agent_discovery(Ok(Vec::new()));
    let (_, rows, selected) = lost.app.palette_view().expect("Commands after target loss");
    assert_eq!(rows[selected], "Copy thought");
}

#[test]
fn commands_show_bindings_and_scope_from_the_captured_invocation_mode() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.board]\n\"clipboard.copy\"=[{key='F5'}]\n\"clipboard.paste_exact\"=[{key='F6'}]\n\"selection.select_all\"=[{key='F10'}]\n[bindings.edit]\n\"clipboard.copy\"=[{key='F7'}]\n\"clipboard.paste_exact\"=[{key='F8'}]\n\"commands.open\"=[{key='F9'}]\n\"selection.select_all\"=[{key='F11'}]",
        )
        .expect("contextual Commands bindings"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    saved_thought(&mut fixture, "contextual shortcuts");

    open(&mut fixture);
    let (_, rendered) = searched_row(&mut fixture, "Copy thought", 88);
    assert!(rendered.contains("F5 · board"));
    fixture.input(crate::key_input(UiKey::Escape));
    open(&mut fixture);
    let (_, rendered) = searched_row(&mut fixture, "Paste exactly", 88);
    assert!(rendered.contains("F6 · board"));
    fixture.input(crate::key_input(UiKey::Escape));

    fixture.input(crate::key_input(UiKey::Character('e')));
    assert!(fixture.app.editor_snapshot().is_some());
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut fixture, "Copy thought", 88);
    assert!(rendered.contains("F7 · edit"));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut fixture, "Paste exactly", 88);
    assert!(rendered.contains("F8 · edit"));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        9,
    ))));
    let (_, rendered) = searched_row(&mut fixture, "Select all thoughts", 88);
    assert!(rendered.contains("F10 · selection"));
    assert!(!rendered.contains("F11"));
}

#[test]
fn editor_clipboard_and_board_selection_commands_are_contextually_applicable() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"commands.open\"=[{key='F9'}]",
        )
        .expect("editor Commands binding"),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    saved_thought(&mut fixture, "selection required");
    fixture.input(crate::key_input(UiKey::Character('e')));

    for command in [
        "Copy thought",
        "Cut thought",
        "Toggle thought selection",
        "Start contiguous range selection",
    ] {
        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
            9,
        ))));
        let (enabled, rendered) = searched_row(&mut fixture, command, 88);
        assert!(!enabled, "{command} must not execute without its context");
        let reason = if command.starts_with("Copy") || command.starts_with("Cut") {
            "Select text in the editor first"
        } else {
            "Available from Board focus"
        };
        assert!(rendered.contains(reason));
        fixture.input(crate::key_input(UiKey::Escape));
    }

    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    for command in ["Copy thought", "Cut thought"] {
        fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
            9,
        ))));
        assert!(searched_row(&mut fixture, command, 88).0);
        fixture.input(crate::key_input(UiKey::Escape));
    }
}
