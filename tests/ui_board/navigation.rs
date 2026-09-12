use super::*;

use proqi::domain::Direction;

pub(super) fn visual(movement: CursorMovement, shifted: bool) -> UiInput {
    crate::key_input(UiKey::Move {
        movement,
        extend_selection: shifted,
    })
}

pub(super) use super::durable_thought;

#[test]
fn keyboard_reordering_wraps_at_both_board_edges() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }
    fixture.input(crate::key_input(UiKey::PrimaryCharacter('J')));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["third", "first", "second"]
    );
    fixture.input(crate::key_input(UiKey::PrimaryCharacter('K')));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
}

#[test]
fn help_is_modal_and_escape_closes_it_without_mutating_the_board() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "unchanged");
    fixture.input(crate::key_input(UiKey::Character('?')));
    assert!(fixture.app.help);
    fixture.input(crate::key_input(UiKey::Character('d')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    fixture.input(crate::key_input(UiKey::Escape));
    assert!(!fixture.app.help);
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "unchanged"
    );
}

#[test]
fn shallow_help_scrolls_to_available_boundaries_without_hidden_history() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "redo candidate");
    fixture.input(crate::key_input(UiKey::Undo));
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Character('?')));
    let _initial = draw(&mut fixture, 42, 8);
    for _ in 0..64 {
        fixture.input(visual(CursorMovement::VisualDown, false));
    }
    let terminal = draw(&mut fixture, 42, 8);
    let bottom = text(terminal.backend().buffer());
    let expected = if cfg!(target_os = "macos") {
        "Extend to last"
    } else {
        "Last thought"
    };
    assert!(bottom.contains(expected));
    assert!(!bottom.contains("Undo") && !bottom.contains("Redo"));
    for _ in 0..7 {
        fixture.pointer(1, 1, PointerKind::ScrollUp);
    }
    let terminal = draw(&mut fixture, 42, 8);
    let scrolled = text(terminal.backend().buffer());
    assert_ne!(scrolled, bottom);
    assert!(!scrolled.contains("Undo") && !scrolled.contains("Redo"));
}

#[test]
fn help_list_uses_identical_arrow_and_jk_navigation() {
    let mut arrow = Fixture::new();
    let mut vim = Fixture::new();
    for fixture in [&mut arrow, &mut vim] {
        fixture.input(crate::key_input(UiKey::Escape));
        fixture.input(crate::key_input(UiKey::Character('?')));
        let _layout = draw(fixture, 42, 8);
    }
    arrow.input(visual(CursorMovement::VisualDown, false));
    vim.input(crate::key_input(UiKey::Character('j')));
    assert_eq!(
        text(draw(&mut arrow, 42, 8).backend().buffer()),
        text(draw(&mut vim, 42, 8).backend().buffer())
    );
    arrow.input(visual(CursorMovement::VisualUp, false));
    vim.input(crate::key_input(UiKey::Character('k')));
    assert_eq!(
        text(draw(&mut arrow, 42, 8).backend().buffer()),
        text(draw(&mut vim, 42, 8).backend().buffer())
    );
}

#[test]
fn modal_navigation_wins_when_help_is_remapped_to_j() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_legacy(&proqi::ui::KeyBindings {
            focus_down: 'g',
            help: 'j',
            ..proqi::ui::KeyBindings::default()
        })
        .expect("valid translated keymap"),
        ..UiSettings::default()
    };
    let mut arrow = Fixture::with_settings(settings.clone());
    let mut vim = Fixture::with_settings(settings);
    for fixture in [&mut arrow, &mut vim] {
        fixture.input(crate::key_input(UiKey::Escape));
        fixture.input(crate::key_input(UiKey::Character('j')));
        let _layout = draw(fixture, 42, 8);
    }

    arrow.input(visual(CursorMovement::VisualDown, true));
    vim.input(crate::key_input(UiKey::PrimaryCharacter('J')));
    assert!(arrow.app.help && vim.app.help);
    assert_eq!(
        text(draw(&mut arrow, 42, 8).backend().buffer()),
        text(draw(&mut vim, 42, 8).backend().buffer())
    );
    let rendered = text(draw(&mut vim, 120, 32).backend().buffer());
    assert!(
        rendered
            .lines()
            .any(|line| line.contains("Esc") && line.contains("Close"))
    );
    assert!(
        !rendered
            .lines()
            .any(|line| line.contains('j') && line.contains("Close"))
    );
    vim.input(crate::key_input(UiKey::Escape));
    assert!(!vim.app.help);
}

#[test]
fn wide_help_uses_at_most_two_strictly_aligned_columns() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    fixture.input(crate::key_input(UiKey::Character('?')));
    let terminal = draw(&mut fixture, 120, 20);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("Submit & keep"));
    let quit = rendered
        .lines()
        .find(|line| line.contains("Quit"))
        .expect("quit row");
    let first_row = rendered
        .lines()
        .find(|line| line.contains("New"))
        .expect("first shortcut row");
    let first_column = first_row.find("New").expect("first column");
    let second_column = first_row.find("Edit").expect("second column");
    let quit_column = quit.find("Quit").expect("quit column");
    assert!(
        [first_column, second_column].contains(&quit_column),
        "{quit}"
    );
}

#[test]
fn insertion_row_keeps_board_commands_available_until_creation_is_explicit() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "existing");
    fixture.input(visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.insertion_focused());
    assert!(fixture.app.active_thought_id().is_none());

    fixture.input(crate::key_input(UiKey::Character('j')));
    assert!(fixture.app.insertion_focused());
    assert!(fixture.app.editor_snapshot().is_none());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    fixture.input(crate::key_input(UiKey::Character(':')));
    assert!(fixture.app.palette_view().is_some());
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Character('n')));
    assert_eq!(
        fixture.app.editor_snapshot().expect("new editor").content,
        ""
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn durable_blank_thought_uses_board_commands_instead_of_implicit_typing() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Character('n')));
    fixture.input(crate::key_input(UiKey::Escape));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    fixture.input(crate::key_input(UiKey::Character('d')));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn blocked_down_navigation_from_the_last_editor_creates_and_edits_a_blank() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "last thought");
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    }));

    fixture.input(visual(CursorMovement::VisualDown, false));
    fixture.input(visual(CursorMovement::VisualDown, false));

    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert_eq!(fixture.app.editor_snapshot().expect("editor").content, "");
}

#[test]
fn two_consecutive_blocked_vertical_moves_leave_edit_mode_for_a_neighbor() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "first");
    durable_thought(&mut fixture, "second");
    let first = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));

    fixture.input(visual(CursorMovement::VisualUp, false));
    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
    fixture.input(visual(CursorMovement::VisualUp, false));
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    );
    assert_eq!(fixture.app.state.focused_thought, Some(first));
}

#[test]
fn other_input_resets_the_blocked_navigation_confirmation() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "first");
    durable_thought(&mut fixture, "second");
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(crate::key_input(UiKey::Character('x')));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(visual(CursorMovement::VisualUp, false));

    assert!(matches!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Edit { .. }
    ));
}

#[test]
fn focused_editor_uses_the_terminal_cursor_without_painting_its_cell() {
    let mut fixture = Fixture::new();
    fixture.paste("cursor");
    let terminal = draw_theme(&mut fixture, 30, 7, ThemePreference::Dark);
    let mut terminal = terminal;
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("visible cursor");
    let cell = &terminal.backend().buffer()[cursor];
    let theme = Theme::resolve(ThemePreference::Dark, true);
    assert_eq!(cell.bg, theme.focused_surface.expect("focused surface"));
    assert_eq!(cell.fg, theme.foreground);
}

#[test]
fn expanded_overflow_reaches_later_thoughts_and_insertion_without_blank_overscroll() {
    let mut fixture = Fixture::new();
    let long = (1..=10)
        .map(|line| format!("first line {line} contains enough text to wrap twice"))
        .collect::<Vec<_>>()
        .join("\n");
    durable_thought(&mut fixture, &long);
    let first = fixture.app.state.board.live_thoughts()[0].id;
    fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
    fixture.input(crate::key_input(UiKey::Character('c')));
    durable_thought(&mut fixture, "final thought");
    fixture.input(crate::key_input(UiKey::Character('k')));
    assert_eq!(fixture.app.state.focused_thought, Some(first));

    for _ in 0..40 {
        fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));
        fixture.pointer(1, 1, PointerKind::ScrollDown);
    }
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 42, 12));

    assert!(layout.thoughts.iter().any(|thought| {
        fixture
            .app
            .state
            .board
            .thought(thought.thought_id)
            .is_some_and(|value| value.content == "final thought")
    }));
    assert!(layout.insert.is_some());
}

#[test]
fn presentation_cycle_is_durable_and_recomputes_overflow_bounds() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, &["line"; 10].join("\n"));
    let initial = fixture.app.prepare_frame(Rect::new(0, 0, 36, 12));
    assert!(
        initial.thoughts[0].hidden_rows > 0,
        "layout: {:?}",
        initial.thoughts[0]
    );

    fixture.input(crate::key_input(UiKey::Character('c')));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].presentation,
        proqi::domain::ThoughtPresentation::Expanded
    );
    fixture.app.prepare_frame(Rect::new(0, 0, 36, 12));
    fixture.input(crate::key_input(UiKey::Character('c')));
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].presentation,
        proqi::domain::ThoughtPresentation::Collapsed
    );
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 36, 12));
    assert!(layout.thoughts[0].overflow.is_some());
}

#[test]
fn drag_handle_uses_the_upper_center_cell_for_every_height() {
    for (height, expected_row) in [(1, 0), (2, 0), (3, 1), (4, 1), (5, 2), (6, 2)] {
        let mut fixture = Fixture::new();
        fixture.paste(&vec!["line"; height].join("\n"));
        let terminal = draw(&mut fixture, 40, 14);
        let thought = &fixture.app.prepare_frame(Rect::new(0, 0, 40, 14)).thoughts[0];
        let handle_row = thought.gutter.y + expected_row;
        assert_eq!(
            terminal.backend().buffer()[(thought.gutter.x, handle_row)].symbol(),
            "⋮",
            "thought height {height}"
        );
    }
}

#[test]
fn mouse_wheel_scrolls_editor_without_moving_cursor_or_selection() {
    let mut fixture = Fixture::new();
    fixture.paste(
        &(0..20)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Enter));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: true,
    }));
    let _terminal = draw(&mut fixture, 30, 6);
    let before = fixture.app.editor_snapshot().expect("editor");
    let text_area = fixture.app.prepare_frame(Rect::new(0, 0, 30, 6)).thoughts[0].text_area;
    fixture.pointer(text_area.x, text_area.y, PointerKind::ScrollDown);
    let after = fixture.app.editor_snapshot().expect("editor");
    assert!(
        after.scroll_row > before.scroll_row,
        "scroll must advance: before={before:?}, after={after:?}, mode={:?}",
        fixture.app.interaction_mode()
    );
    assert_eq!(after.cursor, before.cursor);
    assert_eq!(after.selection, before.selection);
}
