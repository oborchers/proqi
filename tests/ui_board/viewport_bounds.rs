//! Viewport bounds, overscroll clamping, and resize recovery.

use super::{Fixture, draw, durable_thought, text};
use proqi::ui::PointerKind;
use ratatui_core::layout::Rect;

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
fn a_long_thought_scrolls_one_wrapped_row_at_a_time() {
    let mut fixture = Fixture::new();
    durable_thought(
        &mut fixture,
        "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7",
    );
    let area = Rect::new(0, 0, 42, 9);
    let initial = fixture.app.prepare_frame(area);
    assert!(initial.thoughts[0].hidden_rows > 0);
    assert_eq!(initial.first_row_offset, 0);

    fixture.pointer(5, 2, PointerKind::ScrollDown);
    let once = fixture.app.prepare_frame(area);
    assert_eq!(once.first_index, 0);
    assert_eq!(once.first_row_offset, 1);

    fixture.pointer(5, 2, PointerKind::ScrollDown);
    let twice = fixture.app.prepare_frame(area);
    assert_eq!(twice.first_row_offset, 2);
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
    assert!(last_text.contains("+ New thought"));
    let last_layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 9));
    let maximum = last_layout.maximum_viewport_offset;
    assert_eq!(last_layout.viewport_offset, maximum);
    assert!(last_layout.insert.is_some());

    let _large = draw(&mut fixture, 50, 40);
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 50, 40));
    assert_eq!(layout.first_index, 0);
    assert_eq!(layout.max_first_index, 0);
}

#[test]
fn overflowing_board_scrolls_to_the_insertion_row_without_blank_overscroll() {
    let mut fixture = Fixture::new();
    for index in 0..10 {
        durable_thought(&mut fixture, &format!("thought {index}"));
    }
    let area = Rect::new(0, 0, 52, 10);
    let _initial = fixture.app.prepare_frame(area);
    for _ in 0..20 {
        fixture.pointer(5, 3, PointerKind::ScrollDown);
        let _rendered = draw(&mut fixture, area.width, area.height);
    }

    let final_page = fixture.app.prepare_frame(area);
    let insert = final_page.insert.expect("reachable insertion row");
    assert!(insert.bottom() <= final_page.board.bottom());
    assert!(final_page.thoughts.iter().any(|thought| thought.index == 9));
    assert_eq!(final_page.first_index, final_page.max_first_index);

    fixture.pointer(5, 3, PointerKind::ScrollDown);
    let clamped = fixture.app.prepare_frame(area);
    assert_eq!(clamped.first_index, final_page.first_index);
    assert!(clamped.insert.is_some());
}
