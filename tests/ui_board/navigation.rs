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
    assert_eq!(cell.bg, theme.accent);
    assert_eq!(cell.fg, theme.focused_foreground);
}

#[test]
fn drag_handle_is_centered_for_even_and_odd_thought_heights() {
    for content in ["one\ntwo", "one\ntwo\nthree"] {
        let mut fixture = Fixture::new();
        fixture.paste(content);
        let terminal = draw(&mut fixture, 40, 10);
        let thought = &fixture.app.prepare_frame(Rect::new(0, 0, 40, 10)).thoughts[0];
        let center = thought.gutter.y + thought.gutter.height / 2;
        assert_eq!(
            terminal.backend().buffer()[(thought.gutter.x, center)].symbol(),
            "⋮"
        );
    }
}
