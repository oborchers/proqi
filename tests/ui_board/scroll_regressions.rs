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
