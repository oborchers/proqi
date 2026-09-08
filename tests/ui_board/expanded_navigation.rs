//! Board navigation ownership for viewport-clipped expanded thoughts.

use super::*;
use proqi::domain::{ThoughtId, ThoughtPresentation};
use proqi::ui::KeyPhase;

const AREA: Rect = Rect::new(0, 0, 36, 12);

fn long_content(label: &str) -> String {
    (0..10)
        .map(|row| format!("{label} {row:02}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn expanded_fixture(previous: bool, next: bool) -> (Fixture, ThoughtId, String) {
    let mut fixture = Fixture::new();
    if previous {
        durable_thought(&mut fixture, "previous synthetic thought");
    }
    let content = long_content("expanded row");
    durable_thought(&mut fixture, &content);
    let focused = fixture.app.active_thought_id().expect("long thought focus");
    if next {
        durable_thought(&mut fixture, "next synthetic thought");
        fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
    }
    fixture
        .app
        .state
        .board
        .thought_mut(focused)
        .expect("long thought")
        .presentation = ThoughtPresentation::Collapsed;
    let collapsed = fixture.app.prepare_frame(AREA);
    assert!(
        collapsed
            .thought(focused)
            .expect("collapsed thought")
            .hidden_rows
            > 0
    );
    fixture.input(crate::key_input(UiKey::Character('c')));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .presentation,
        ThoughtPresentation::Expanded
    );
    (fixture, focused, content)
}

fn span(fixture: &mut Fixture, focused: ThoughtId, area: Rect) -> (usize, usize, usize) {
    let frame = fixture.app.prepare_frame(area);
    let thought = frame.thought(focused).expect("focused thought visible");
    let first = thought.content_row_offset;
    let end = first + usize::from(thought.text_area.height);
    let content = &fixture
        .app
        .state
        .board
        .thought(focused)
        .expect("thought")
        .content;
    let total = content.split('\n').count();
    (first, end, total)
}

fn move_to_bottom(fixture: &mut Fixture, focused: ThoughtId, area: Rect) {
    for _ in 0..256 {
        let before = span(fixture, focused, area);
        if before.1 >= before.2 {
            return;
        }
        assert!(
            fixture
                .effects(super::navigation::visual(CursorMovement::VisualDown, false))
                .is_empty()
        );
        let after = span(fixture, focused, area);
        assert_eq!(fixture.app.active_thought_id(), Some(focused));
        assert_eq!(after.1, before.1 + 1);
    }
    panic!("expanded thought did not reach its bottom boundary");
}

fn move_to_top(fixture: &mut Fixture, focused: ThoughtId, area: Rect) {
    for _ in 0..256 {
        let before = span(fixture, focused, area);
        if before.0 == 0 {
            return;
        }
        fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
        let after = span(fixture, focused, area);
        assert_eq!(fixture.app.active_thought_id(), Some(focused));
        assert_eq!(after.0 + 1, before.0);
    }
    panic!("expanded thought did not reach its top boundary");
}

#[test]
fn expanded_thought_consumes_every_row_then_crosses_both_focus_boundaries() {
    let (mut down, focused, content) = expanded_fixture(true, true);
    let durable_before = down
        .app
        .state
        .board
        .thought(focused)
        .expect("thought")
        .clone();
    let history_before = down.app.state.board_history_cursor();
    move_to_bottom(&mut down, focused, AREA);
    let bottom = span(&mut down, focused, AREA);
    assert_eq!(bottom.1, bottom.2);
    assert_eq!(
        down.app.state.board.thought(focused).expect("thought"),
        &durable_before
    );
    assert_eq!(down.app.state.board_history_cursor(), history_before);
    assert!(super::movement_symmetry::selected(&down).is_empty());
    assert!(
        down.effects(super::navigation::visual(CursorMovement::VisualDown, false,))
            .is_empty()
    );
    assert_eq!(
        down.app.active_thought_id(),
        Some(down.app.state.board.live_thoughts()[2].id)
    );
    assert_eq!(
        down.app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .content,
        content
    );

    let (mut up, focused, content) = expanded_fixture(true, true);
    let history_before = up.app.state.board_history_cursor();
    move_to_bottom(&mut up, focused, AREA);
    move_to_top(&mut up, focused, AREA);
    assert_eq!(span(&mut up, focused, AREA).0, 0);
    assert_eq!(up.app.state.board_history_cursor(), history_before);
    assert!(
        up.effects(super::navigation::visual(CursorMovement::VisualUp, false))
            .is_empty()
    );
    assert_eq!(
        up.app.active_thought_id(),
        Some(up.app.state.board.live_thoughts()[0].id)
    );
    assert_eq!(
        up.app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .content,
        content
    );
}

#[test]
fn neighbor_shapes_keep_internal_ownership_and_existing_outer_boundaries() {
    for (previous, next) in [(true, false), (false, true), (true, true)] {
        let (mut fixture, focused, _) = expanded_fixture(previous, next);
        let initial = span(&mut fixture, focused, AREA);
        fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
        let after = span(&mut fixture, focused, AREA);
        assert_eq!(fixture.app.active_thought_id(), Some(focused));
        assert_eq!(after.1, initial.1 + 1, "previous={previous}, next={next}");

        move_to_bottom(&mut fixture, focused, AREA);
        fixture.input(super::navigation::visual(CursorMovement::VisualDown, false));
        if next {
            assert_ne!(fixture.app.active_thought_id(), Some(focused));
        } else {
            assert!(fixture.app.insertion_focused());
        }
    }
}

#[test]
fn arrows_aliases_repeat_and_mouse_use_the_same_single_row_step() {
    fn bottom_then_up(input: UiInput) -> (usize, usize, usize) {
        let (mut fixture, focused, _) = expanded_fixture(true, true);
        move_to_bottom(&mut fixture, focused, AREA);
        fixture.input(input);
        span(&mut fixture, focused, AREA)
    }

    let arrow = bottom_then_up(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Up)));
    let alias = bottom_then_up(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
        'k',
    ))));
    let mut repeat = KeyStroke::press(LogicalKey::Up);
    repeat.phase = KeyPhase::Repeat;
    let repeated = bottom_then_up(UiInput::KeyStroke(repeat));

    let (mut mouse, focused, _) = expanded_fixture(true, true);
    move_to_bottom(&mut mouse, focused, AREA);
    mouse.pointer(4, 2, PointerKind::ScrollUp);
    let wheel = span(&mut mouse, focused, AREA);
    assert_eq!(alias, arrow);
    assert_eq!(repeated, arrow);
    assert_eq!(wheel, arrow);

    let before = span(&mut mouse, focused, AREA);
    for _ in 0..12 {
        mouse.input(super::navigation::visual(CursorMovement::VisualDown, false));
        let _ = mouse.app.prepare_frame(AREA);
        mouse.input(super::navigation::visual(CursorMovement::VisualUp, false));
        let _ = mouse.app.prepare_frame(AREA);
    }
    assert_eq!(span(&mut mouse, focused, AREA), before);
}

#[test]
fn shifted_range_and_reorder_intentions_do_not_become_internal_scroll() {
    let (mut range, focused, content) = expanded_fixture(true, true);
    move_to_bottom(&mut range, focused, AREA);
    let before = span(&mut range, focused, AREA);
    range.input(super::navigation::visual(CursorMovement::VisualUp, true));
    assert_eq!(
        super::movement_symmetry::selected(&range),
        ["previous synthetic thought", content.as_str()]
    );
    assert_ne!(range.app.active_thought_id(), Some(focused));

    let (mut reorder, focused, _) = expanded_fixture(true, true);
    move_to_bottom(&mut reorder, focused, AREA);
    reorder.input(crate::key_input(UiKey::PrimaryShiftMove {
        movement: CursorMovement::VisualUp,
    }));
    assert_eq!(reorder.app.state.board.live_thoughts()[0].id, focused);
    assert_eq!(
        reorder
            .app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .presentation,
        ThoughtPresentation::Expanded
    );
    assert!(before.0 > 0);
}

#[test]
fn resize_reflow_keeps_expanded_state_valid_while_internally_scrolled() {
    let (mut fixture, focused, content) = expanded_fixture(true, true);
    move_to_bottom(&mut fixture, focused, AREA);
    fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
    for area in [
        Rect::new(0, 0, 24, 8),
        Rect::new(0, 0, 64, 8),
        Rect::new(0, 0, 28, 18),
        Rect::new(0, 0, 80, 20),
        Rect::new(0, 0, 24, 8),
    ] {
        let current = span(&mut fixture, focused, area);
        assert!(current.0 < current.2);
        assert!(current.1 <= current.2);
        assert_eq!(fixture.app.active_thought_id(), Some(focused));
        assert_eq!(
            fixture
                .app
                .state
                .board
                .thought(focused)
                .expect("thought")
                .presentation,
            ThoughtPresentation::Expanded
        );
    }
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(focused)
            .expect("thought")
            .content,
        content
    );
}

#[test]
fn collapsed_and_automatic_presentations_keep_thought_level_navigation() {
    for preference in [
        ThoughtPresentation::Collapsed,
        ThoughtPresentation::Automatic,
    ] {
        let (mut fixture, focused, _) = expanded_fixture(true, true);
        fixture
            .app
            .state
            .board
            .thought_mut(focused)
            .expect("thought")
            .presentation = preference;
        let _ = fixture.app.prepare_frame(AREA);
        fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
        assert_ne!(
            fixture.app.active_thought_id(),
            Some(focused),
            "{preference:?}"
        );
    }
}

#[test]
fn internally_scrolled_expanded_thought_has_a_reviewable_snapshot() {
    let (mut fixture, focused, _) = expanded_fixture(true, true);
    move_to_bottom(&mut fixture, focused, AREA);
    fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
    let terminal = draw(&mut fixture, AREA.width, AREA.height);
    insta::assert_snapshot!(super::snapshot_support::snapshot_buffer(
        terminal.backend().buffer()
    ));
}
