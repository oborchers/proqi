use super::*;

#[test]
fn repeated_down_navigation_creates_only_one_blank_thought() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(&mut fixture, "last thought");
    fixture.input(UiInput::Key(UiKey::Enter));
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentEnd,
        extend_selection: false,
    }));

    for _ in 0..6 {
        fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    }

    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    assert_eq!(fixture.app.editor_snapshot().expect("editor").content, "");
}

#[test]
fn reverse_scroll_into_a_collapsed_thought_starts_at_its_preview() {
    let mut fixture = Fixture::new();
    super::navigation::durable_thought(
        &mut fixture,
        "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7",
    );
    fixture.app.prepare_frame(Rect::new(0, 0, 42, 8));
    fixture.input(UiInput::Key(UiKey::Character('c')));
    fixture.app.prepare_frame(Rect::new(0, 0, 42, 8));
    fixture.input(UiInput::Key(UiKey::Character('c')));
    super::navigation::durable_thought(&mut fixture, "following thought");
    let area = Rect::new(0, 0, 42, 8);
    let _initial = fixture.app.prepare_frame(area);

    for _ in 0..8 {
        fixture.pointer(5, 2, PointerKind::ScrollDown);
        let _rendered = fixture.app.prepare_frame(area);
    }
    assert_eq!(fixture.app.prepare_frame(area).first_index, 1);

    fixture.pointer(5, 2, PointerKind::ScrollUp);
    let previous = fixture.app.prepare_frame(area);
    assert_eq!(previous.first_index, 0);
    assert_eq!(previous.first_row_offset, 0);
    assert!(previous.thoughts[0].overflow.is_some());
}

#[test]
fn repeated_long_thought_cycles_resize_and_scroll_to_both_board_boundaries() {
    let mut fixture = Fixture::new();
    let long_a = (0..10)
        .map(|line| format!("A {line} wraps with Grüße 界 and enough words for narrow panes"))
        .collect::<Vec<_>>()
        .join("\n");
    let long_b = (0..10)
        .map(|line| format!("B {line} wraps with emoji 🧪 and enough words for narrow panes"))
        .collect::<Vec<_>>()
        .join("\n");
    for content in ["before", &long_a, "between", &long_b, "after"] {
        super::navigation::durable_thought(&mut fixture, content);
    }
    let small = Rect::new(0, 0, 30, 9);
    let _initial = fixture.app.prepare_frame(small);
    for _ in 0..3 {
        fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
    }
    let _focused_long_a = fixture.app.prepare_frame(small);
    fixture.input(UiInput::Key(UiKey::Character('c')));
    for _ in 0..3 {
        fixture.input(UiInput::Key(UiKey::Character('c')));
        let _collapsed = fixture.app.prepare_frame(small);
        fixture.input(UiInput::Key(UiKey::Character('c')));
        let expanded = fixture.app.prepare_frame(small);
        let active = fixture.app.active_thought_id().expect("long thought focus");
        assert!(
            expanded
                .thought(active)
                .expect("visible long thought")
                .overflow
                .is_none()
        );
    }
    for _ in 0..2 {
        fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
    }
    let _long_b = fixture.app.prepare_frame(small);
    fixture.input(UiInput::Key(UiKey::Character('c')));
    for area in [
        Rect::new(0, 0, 28, 8),
        Rect::new(0, 0, 72, 18),
        Rect::new(0, 0, 34, 7),
        Rect::new(0, 0, 50, 20),
    ] {
        let layout = fixture.app.prepare_frame(area);
        let active = fixture.app.active_thought_id().expect("focus after resize");
        assert!(layout.thought(active).is_some());
    }

    for _ in 0..240 {
        let _frame = fixture.app.prepare_frame(small);
        fixture.pointer(4, 2, PointerKind::ScrollDown);
    }
    let bottom = fixture.app.prepare_frame(small);
    assert!(bottom.insert.is_some());
    assert!(bottom.thoughts.iter().any(|thought| thought.index == 4));
    let bottom_anchor = (bottom.first_index, bottom.first_row_offset);
    fixture.pointer(4, 2, PointerKind::ScrollDown);
    let clamped = fixture.app.prepare_frame(small);
    assert_eq!(
        (clamped.first_index, clamped.first_row_offset),
        bottom_anchor
    );

    for _ in 0..240 {
        let _frame = fixture.app.prepare_frame(small);
        fixture.pointer(4, 2, PointerKind::ScrollUp);
    }
    let top = fixture.app.prepare_frame(small);
    assert_eq!((top.first_index, top.first_row_offset), (0, 0));
    assert_eq!(fixture.app.state.board.live_thoughts()[1].content, long_a);
    assert_eq!(fixture.app.state.board.live_thoughts()[3].content, long_b);
}
