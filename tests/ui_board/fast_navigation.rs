use super::*;
use proqi::domain::TextPosition;
use proqi::ui::FastNavigation;

fn move_cursor(fixture: &mut Fixture, movement: CursorMovement) {
    fixture.input(crate::key_input(UiKey::Move {
        movement,
        extend_selection: false,
    }));
}

fn fast(fixture: &mut Fixture, direction: FastNavigation, extend_selection: bool) {
    fixture.input(crate::key_input(UiKey::FastNavigation {
        direction,
        extend_selection,
    }));
}

#[test]
fn command_palette_fast_navigation_moves_five_entries_and_clamps() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    fixture.input(crate::key_input(UiKey::Character(':')));
    let (_, all, _) = fixture.app.palette_view().expect("palette");
    let expected = all[5].clone();
    let _ = draw(&mut fixture, 38, 6);
    fast(&mut fixture, FastNavigation::Next, false);
    let (_, visible, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(visible[selected], expected);

    for _ in 0..20 {
        fast(&mut fixture, FastNavigation::Next, false);
    }
    let (_, visible, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(visible[selected], "More commands...");
}

#[test]
fn command_palette_wheel_is_contained_and_retargets_the_visible_slice() {
    let mut fixture = Fixture::new();
    for index in 0..12 {
        navigation::durable_thought(&mut fixture, &format!("thought {index}"));
    }
    let before = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 36, 7))
        .first_index;
    fixture.input(crate::key_input(UiKey::Character(':')));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 36, 7));
    let item = layout.overlay.expect("overlay").items[0];
    fixture.pointer(item.x, item.y, PointerKind::ScrollDown);
    let _ = draw(&mut fixture, 36, 7);
    let (_, visible, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(visible[selected], "Undo board action");
    fixture.input(crate::key_input(UiKey::Escape));
    assert_eq!(
        fixture
            .app
            .prepare_frame(Rect::new(0, 0, 36, 7))
            .first_index,
        before
    );
}

#[test]
fn help_fast_navigation_moves_exactly_five_visible_rows() {
    let mut paged = Fixture::new();
    let mut repeated = Fixture::new();
    for fixture in [&mut paged, &mut repeated] {
        fixture.input(crate::key_input(UiKey::Escape));
        fixture.input(crate::key_input(UiKey::Character('?')));
        let initial = draw(fixture, 42, 8);
        assert!(text(initial.backend().buffer()).contains('↓'));
    }
    fast(&mut paged, FastNavigation::Next, false);
    for _ in 0..5 {
        repeated.input(navigation::visual(CursorMovement::VisualDown, false));
    }
    assert_eq!(
        text(draw(&mut paged, 42, 8).backend().buffer()),
        text(draw(&mut repeated, 42, 8).backend().buffer())
    );
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
    fixture.input(crate::key_input(UiKey::Enter));
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

    fixture.input(crate::key_input(UiKey::Escape));
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
fn mouse_reposition_sets_the_column_for_the_next_fast_jump() {
    let content = (0..10)
        .map(|row| format!("row {row}: 0123456789abcdefghijklmnopqrstuvwxyz界e\u{301}🙂"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut fixture = Fixture::new();
    fixture.paste(&content);
    move_cursor(&mut fixture, CursorMovement::DocumentStart);
    let _initial = draw(&mut fixture, 80, 16);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 80, 16)).thoughts[0].text_area;

    fixture.pointer(
        area.x.saturating_add(35),
        area.y,
        PointerKind::Down(PointerButton::Left),
    );
    fixture.pointer(
        area.x.saturating_add(35),
        area.y,
        PointerKind::Up(PointerButton::Left),
    );
    move_cursor(&mut fixture, CursorMovement::VisualDown);
    let _moved = draw(&mut fixture, 80, 16);
    fixture.pointer(
        area.x.saturating_add(4),
        area.y.saturating_add(2),
        PointerKind::Down(PointerButton::Left),
    );
    fixture.pointer(
        area.x.saturating_add(4),
        area.y.saturating_add(2),
        PointerKind::Up(PointerButton::Left),
    );
    move_cursor(&mut fixture, CursorMovement::VisualJumpDown);

    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        TextPosition::new(7, 4)
    );
}

#[test]
fn platform_insertion_default_keeps_editor_alt_fast_movement() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        navigation::durable_thought(&mut fixture, content);
    }
    let insertion = if cfg!(target_os = "macos") {
        KeyStroke::press(LogicalKey::Character('n'))
            .with_modifiers(LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT))
    } else {
        KeyStroke::press(LogicalKey::Up).with_modifiers(LogicalModifiers::ALT)
    };
    fixture.input(UiInput::KeyStroke(insertion));
    assert_eq!(fixture.app.state.board.live_thoughts()[2].content, "");
    assert_eq!(
        fixture
            .app
            .editor_snapshot()
            .expect("inserted editor")
            .content,
        ""
    );

    let mut editor = Fixture::new();
    navigation::durable_thought(&mut editor, "zero\none\ntwo\nthree\nfour\nfive\nsix");
    editor.input(crate::key_input(UiKey::Enter));
    move_cursor(&mut editor, CursorMovement::DocumentStart);
    editor.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::Down).with_modifiers(LogicalModifiers::ALT),
    ));
    assert_eq!(
        editor.app.editor_snapshot().expect("editor").cursor,
        TextPosition::new(5, 0)
    );
}

#[test]
fn normalized_fast_intention_moves_and_selects_exactly_five_wrapped_rows() {
    let mut fixture = Fixture::new();
    fixture.paste("0123456789界🙂 alpha beta gamma delta epsilon zeta eta theta");
    let _ = draw(&mut fixture, 14, 8);
    move_cursor(&mut fixture, CursorMovement::DocumentStart);
    let before = fixture.app.editor_snapshot().expect("editor").cursor;
    fast(&mut fixture, FastNavigation::Next, true);
    let selected = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(selected.selection.map(|range| range.start), Some(before));

    let mut comparison = Fixture::new();
    comparison.paste("0123456789界🙂 alpha beta gamma delta epsilon zeta eta theta");
    let _ = draw(&mut comparison, 14, 8);
    move_cursor(&mut comparison, CursorMovement::DocumentStart);
    move_cursor(&mut comparison, CursorMovement::VisualJumpDown);
    assert_eq!(
        selected.cursor,
        comparison.app.editor_snapshot().expect("comparison").cursor
    );
}

#[test]
fn board_fast_navigation_moves_and_selects_exactly_five_thoughts() {
    let mut fixture = Fixture::new();
    for content in [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
        "tenth", "eleventh", "twelfth",
    ] {
        navigation::durable_thought(&mut fixture, content);
    }
    fast(&mut fixture, FastNavigation::Previous, false);
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[6].id)
    );

    fast(&mut fixture, FastNavigation::Previous, true);
    let selected = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .filter(|thought| fixture.app.thought_selected(thought.id))
        .map(|thought| thought.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        selected,
        ["second", "third", "fourth", "fifth", "sixth", "seventh"]
    );
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[1].id)
    );
}

#[test]
fn board_fast_navigation_clamps_and_respects_the_insertion_boundary() {
    let mut fixture = Fixture::new();
    for content in [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh",
    ] {
        navigation::durable_thought(&mut fixture, content);
    }

    fast(&mut fixture, FastNavigation::Next, false);
    assert!(!fixture.app.insertion_focused());
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[6].id)
    );
    fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    assert!(fixture.app.insertion_focused());
    fast(&mut fixture, FastNavigation::Previous, true);
    assert!(fixture.app.insertion_focused());
    assert!(super::movement_symmetry::selected(&fixture).is_empty());

    fast(&mut fixture, FastNavigation::Previous, false);
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[2].id)
    );
    fast(&mut fixture, FastNavigation::Previous, false);
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[0].id)
    );
}

#[test]
fn shifted_page_up_reaches_board_as_a_five_thought_range() {
    let mut fixture = Fixture::new();
    for content in [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
    ] {
        navigation::durable_thought(&mut fixture, content);
    }

    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::PageUp).with_modifiers(LogicalModifiers::SHIFT),
    ));

    assert_eq!(
        super::movement_symmetry::selected(&fixture),
        ["third", "fourth", "fifth", "sixth", "seventh", "eighth"]
    );
    assert_eq!(
        fixture.app.state.focused_thought,
        Some(fixture.app.state.board.live_thoughts()[2].id)
    );
}

#[test]
fn shifted_page_down_reaches_editor_as_a_five_row_selection() {
    let mut fixture = Fixture::new();
    fixture.paste("one\ntwo\nthree\nfour\nfive\nsix\nseven");
    move_cursor(&mut fixture, CursorMovement::DocumentStart);

    fixture.input(UiInput::KeyStroke(
        KeyStroke::press(LogicalKey::PageDown).with_modifiers(LogicalModifiers::SHIFT),
    ));

    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(snapshot.cursor, TextPosition::new(5, 0));
    assert_eq!(
        snapshot.selection,
        Some(proqi::ports::editor::TextSelection {
            start: TextPosition::new(0, 0),
            end: TextPosition::new(5, 0),
        })
    );
}

#[test]
fn contextual_help_uses_platform_primary_labels_for_fast_navigation() {
    let settings = UiSettings {
        shortcuts: proqi::ui::ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"help.open\"=[{key='F5'}]",
        )
        .unwrap(),
        ..UiSettings::default()
    };
    let mut fixture = Fixture::with_settings(settings);
    let sequence = fixture.paste("one\ntwo\nthree\nfour\nfive\nsix");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Function(
        5,
    ))));
    let terminal = draw(&mut fixture, 120, 14);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains(if cfg!(target_os = "macos") {
        "Option+↑/↓"
    } else {
        "Alt+↑/↓"
    }));
    assert!(rendered.contains("Move 5 rows"));
    assert!(rendered.contains("Ctrl+↑/↓"));
    assert!(rendered.contains("Start/end"));
    let visual_row = if cfg!(target_os = "macos") {
        "Cmd+Shift+H/←/L/→"
    } else {
        "Ctrl+Shift+H/L"
    };
    assert!(rendered.contains(visual_row));
    assert!(rendered.contains("Select visual row"));
}
