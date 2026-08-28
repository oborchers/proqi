use super::*;
use proqi::domain::TextPosition;

fn move_cursor(fixture: &mut Fixture, movement: CursorMovement) {
    fixture.input(UiInput::Key(UiKey::Move {
        movement,
        extend_selection: false,
    }));
}

fn wrapped_thought(label: &str) -> String {
    (0..10)
        .map(|row| format!("{label} row {row}: abcdefghijklmnopqrstuvwxyz 界 e\u{301} 🙂"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn repeated_fast_jumps_keep_the_cursor_visible_through_internal_scroll_and_resize() {
    let mut fixture = Fixture::new();
    for label in ["first", "second", "third"] {
        navigation::durable_thought(&mut fixture, &wrapped_thought(label));
    }
    fixture.input(UiInput::Key(UiKey::Enter));
    let active = fixture.app.active_thought_id().expect("active thought");
    let _initial = draw(&mut fixture, 30, 8);
    move_cursor(&mut fixture, CursorMovement::DocumentStart);
    for _ in 0..4 {
        move_cursor(&mut fixture, CursorMovement::VisualJumpDown);
    }
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert!(snapshot.scroll_row > 0);
    let mut terminal = draw(&mut fixture, 30, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("visible cursor");
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 30, 8)).thoughts[0].text_area;
    assert!(area.contains(cursor));

    let _narrow = draw(&mut fixture, 18, 6);
    move_cursor(&mut fixture, CursorMovement::VisualJumpUp);
    let mut resized = draw(&mut fixture, 18, 6);
    let cursor = resized
        .backend_mut()
        .get_cursor_position()
        .expect("visible resized cursor");
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 18, 6)).thoughts[0].text_area;
    assert!(area.contains(cursor));

    fixture.input(UiInput::Key(UiKey::Escape));
    assert_eq!(fixture.app.state.focused_thought, Some(active));
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    );
}

#[test]
fn mouse_selection_collapses_before_a_fast_jump() {
    let mut fixture = Fixture::new();
    fixture.paste(&wrapped_thought("mouse"));
    move_cursor(&mut fixture, CursorMovement::DocumentStart);
    let _initial = draw(&mut fixture, 36, 9);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 36, 9)).thoughts[0].text_area;
    fixture.pointer(
        area.x.saturating_add(2),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    let _pressed = draw(&mut fixture, 36, 9);
    fixture.pointer(
        area.x.saturating_add(2),
        area.y,
        PointerKind::Up(PointerButton::Left),
    );
    let _released = draw(&mut fixture, 36, 9);
    fixture.input(UiInput::Pointer(PointerInput {
        column: area.x.saturating_add(8),
        row: area.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: true,
    }));
    let _extended = draw(&mut fixture, 36, 9);
    fixture.input(UiInput::Pointer(PointerInput {
        column: area.x.saturating_add(8),
        row: area.y,
        kind: PointerKind::Up(PointerButton::Left),
        extend_selection: true,
    }));
    let before = fixture.app.editor_snapshot().expect("selection");
    assert!(before.selection.is_some());

    move_cursor(&mut fixture, CursorMovement::VisualJumpDown);
    let after = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(after.selection, None);
    assert_ne!(after.cursor, before.cursor);
}

#[test]
fn mode_aware_alt_navigation_keeps_board_focus_movement_unchanged() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        navigation::durable_thought(&mut fixture, content);
    }
    fixture.input(UiInput::Key(UiKey::EditNavigation {
        editor_movement: CursorMovement::VisualJumpUp,
        board_movement: CursorMovement::VisualUp,
    }));
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[1].id)
    );

    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::EditNavigation {
        editor_movement: CursorMovement::DocumentEnd,
        board_movement: CursorMovement::VisualDown,
    }));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        TextPosition::new(0, 6)
    );
}
