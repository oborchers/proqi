use super::*;

fn visual(movement: CursorMovement, shifted: bool) -> UiInput {
    UiInput::Key(UiKey::Move {
        movement,
        extend_selection: shifted,
    })
}

fn durable_thought(fixture: &mut Fixture, content: &str) {
    fixture.paste(content);
    fixture.input(UiInput::Key(UiKey::Escape));
}

#[test]
fn arrows_and_jk_share_focus_and_reorder_intentions_on_the_board() {
    let mut arrows = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut arrows, content);
    }
    arrows.input(visual(CursorMovement::VisualUp, false));
    let arrow_focus = arrows.app.state.focused_thought;

    let mut letters = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut letters, content);
    }
    letters.input(UiInput::Key(UiKey::Character('k')));
    assert_eq!(letters.app.state.focused_thought, arrow_focus);

    arrows.input(visual(CursorMovement::VisualUp, true));
    letters.input(UiInput::Key(UiKey::Character('K')));
    let arrow_order = arrows
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect::<Vec<_>>();
    let letter_order = letters
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(letter_order, arrow_order);
    assert_eq!(letter_order, ["second", "first", "third"]);
}

#[test]
fn insertion_row_is_keyboard_focusable_and_printable_keys_start_content() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "existing");
    fixture.input(visual(CursorMovement::VisualDown, false));
    assert!(fixture.app.insertion_focused());
    assert!(fixture.app.active_thought_id().is_none());

    fixture.input(UiInput::Key(UiKey::Character('j')));
    assert_eq!(
        fixture.app.editor_snapshot().expect("draft editor").content,
        "j"
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

#[test]
fn two_consecutive_blocked_vertical_moves_leave_edit_mode_for_a_neighbor() {
    let mut fixture = Fixture::new();
    durable_thought(&mut fixture, "first");
    durable_thought(&mut fixture, "second");
    let first = fixture.app.state.board.live_thoughts()[0].id;
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
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
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    fixture.input(visual(CursorMovement::VisualUp, false));
    fixture.input(UiInput::Key(UiKey::Character('x')));
    fixture.input(UiInput::Key(UiKey::Move {
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
fn focused_editor_cursor_cell_uses_the_accent_surface() {
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
    assert_eq!(cell.bg, theme.accent_surface);
    assert_eq!(cell.fg, theme.on_accent);
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
fn an_underfilled_board_does_not_scroll_away_its_content() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        durable_thought(&mut fixture, content);
    }
    let initial = draw(&mut fixture, 60, 24);
    let initial_text = text(initial.backend().buffer());

    for _ in 0..12 {
        fixture.pointer(5, 5, PointerKind::ScrollDown);
        let _rendered = draw(&mut fixture, 60, 24);
    }

    let final_frame = draw(&mut fixture, 60, 24);
    assert_eq!(text(final_frame.backend().buffer()), initial_text);
    assert_eq!(
        fixture
            .app
            .prepare_frame(Rect::new(0, 0, 60, 24))
            .first_index,
        0
    );
}

#[test]
fn overflowing_board_clamps_to_a_useful_last_page_and_resets_after_resize() {
    let mut fixture = Fixture::new();
    for index in 0..8 {
        durable_thought(&mut fixture, &format!("thought {index}"));
    }
    let _small = draw(&mut fixture, 50, 9);
    for _ in 0..20 {
        fixture.pointer(5, 3, PointerKind::ScrollDown);
        let _rendered = draw(&mut fixture, 50, 9);
    }
    let last_page = draw(&mut fixture, 50, 9);
    let last_text = text(last_page.backend().buffer());
    assert!(last_text.contains("thought 7"));
    assert!(last_text.contains("thought 6"));

    let _large = draw(&mut fixture, 50, 40);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 40));
    assert_eq!(layout.first_index, 0);
    assert_eq!(layout.max_first_index, 0);
}

#[test]
fn current_session_can_be_renamed_from_the_palette_and_footer() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "rename session".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.session_rename_view(), Some(""));
    for character in "Agent research".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let effects = fixture.effects(UiInput::Key(UiKey::Enter));
    assert!(matches!(
        effects.as_slice(),
        [Effect::RenameSession { name: Some(name), .. }] if name == "Agent research"
    ));
    assert_eq!(
        fixture.app.state.board.session.name.as_deref(),
        Some("Agent research")
    );
    fixture.app.complete_session_rename(None, Ok(()));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 70, 10));
    let (_, area) = layout
        .controls
        .iter()
        .find(|(target, _)| *target == HitTarget::RenameSession)
        .expect("rename target");
    fixture.pointer(area.x, area.y, PointerKind::Down(PointerButton::Left));
    assert_eq!(fixture.app.session_rename_view(), Some("Agent research"));
}

#[test]
fn failed_session_rename_restores_the_previous_durable_name() {
    let mut fixture = Fixture::new();
    fixture.app.state.board.session.name = Some("Durable".to_owned());
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "rename session".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.input(UiInput::Key(UiKey::Enter));
    for _ in 0.."Durable".len() {
        fixture.input(UiInput::Key(UiKey::Backspace));
    }
    fixture.input(UiInput::Key(UiKey::Character('N')));
    let _effects = fixture.effects(UiInput::Key(UiKey::Enter));
    fixture.app.complete_session_rename(
        Some("Durable".to_owned()),
        Err(proqi::ports::store::StoreError::Busy),
    );
    assert_eq!(
        fixture.app.state.board.session.name.as_deref(),
        Some("Durable")
    );
    assert!(
        fixture
            .app
            .status
            .as_deref()
            .is_some_and(|status| status.contains("failed"))
    );
}
