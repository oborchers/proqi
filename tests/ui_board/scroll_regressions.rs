use super::*;
use proqi::domain::ThoughtPresentation;

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

#[test]
fn one_wheel_step_moves_shared_thought_geometry_by_at_most_one_row() {
    let mut fixture = Fixture::new();
    let long_a = (0..10)
        .map(|line| format!("A {line:02} wraps with Grüße 界 and enough words for narrow panes"))
        .collect::<Vec<_>>()
        .join("\n");
    let long_b = (0..10)
        .map(|line| format!("B {line:02} wraps with emoji 🧪 and enough words for narrow panes"))
        .collect::<Vec<_>>()
        .join("\n");
    for content in ["before", &long_a, "between", &long_b, "after"] {
        super::navigation::durable_thought(&mut fixture, content);
    }
    for index in [1, 3] {
        let id = fixture.app.state.board.live_thoughts()[index].id;
        fixture
            .app
            .state
            .board
            .thought_mut(id)
            .expect("long thought")
            .presentation = proqi::domain::ThoughtPresentation::Expanded;
    }
    for _ in 0..3 {
        fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
    }
    let area = Rect::new(0, 0, 69, 34);

    let mut previous = fixture.app.prepare_frame(area);
    for _ in 0..160 {
        fixture.pointer(4, 2, PointerKind::ScrollDown);
        let next = fixture.app.prepare_frame(area);
        for before in &previous.thoughts {
            let Some(after) = next.thought(before.thought_id) else {
                continue;
            };
            assert!(
                before.area.y.saturating_sub(after.area.y) <= 1,
                "one wheel event jumped thought {} from row {} to {}; before={previous:#?}; after={next:#?}",
                before.index,
                before.area.y,
                after.area.y,
            );
        }
        previous = next;
    }
}

#[test]
fn overflowing_final_page_is_bottom_filled_from_the_tail_of_the_previous_thought() {
    let mut fixture = Fixture::new();
    let long = (0..10)
        .map(|line| format!("long {line:02} wraps with Grüße 界 and enough words for the viewport"))
        .collect::<Vec<_>>()
        .join("\n");
    for content in ["before", &long, "after"] {
        super::navigation::durable_thought(&mut fixture, content);
    }
    let long_id = fixture.app.state.board.live_thoughts()[1].id;
    fixture
        .app
        .state
        .board
        .thought_mut(long_id)
        .expect("long thought")
        .presentation = proqi::domain::ThoughtPresentation::Expanded;
    fixture.input(super::navigation::visual(CursorMovement::VisualUp, false));
    let area = Rect::new(0, 0, 52, 16);

    for _ in 0..160 {
        fixture.pointer(4, 2, PointerKind::ScrollDown);
        let _frame = fixture.app.prepare_frame(area);
    }
    let bottom = fixture.app.prepare_frame(area);
    let insert = bottom.insert.expect("final insertion row");

    assert_eq!(insert.bottom(), bottom.board.bottom(), "{bottom:#?}");
    assert_eq!(
        bottom.thoughts.first().map(|thought| thought.index),
        Some(1)
    );
    assert!(bottom.first_row_offset > 0, "{bottom:#?}");
    assert!(bottom.thoughts.iter().any(|thought| thought.index == 2));
}

#[test]
fn multiple_long_thought_presentation_matrix_keeps_board_pointer_scroll_continuous() {
    let contents = [
        board_long_content("alpha"),
        "ordinary between alpha and beta".to_owned(),
        board_long_content("beta"),
        "ordinary between beta and gamma".to_owned(),
        board_long_content("gamma"),
    ];
    let area = Rect::new(0, 0, 34, 11);
    for presentations in board_presentation_combinations() {
        assert_board_combination(&contents, presentations, area);
    }
}

fn board_long_content(label: &str) -> String {
    (0..18)
        .map(|line| {
            format!(
                "{label} {line:02} · Grüße 界 🧪 · enough ordinary words to reflow in narrow panes"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn board_presentation_combinations() -> Vec<[ThoughtPresentation; 3]> {
    let values = [
        ThoughtPresentation::Automatic,
        ThoughtPresentation::Collapsed,
        ThoughtPresentation::Expanded,
    ];
    values
        .into_iter()
        .flat_map(|first| {
            values
                .into_iter()
                .flat_map(move |second| values.into_iter().map(move |third| [first, second, third]))
        })
        .collect()
}

fn assert_board_combination(
    contents: &[String; 5],
    presentations: [ThoughtPresentation; 3],
    area: Rect,
) {
    let mut fixture = Fixture::new();
    for content in contents {
        super::navigation::durable_thought(&mut fixture, content);
    }
    for (index, presentation) in [
        (0, presentations[0]),
        (2, presentations[1]),
        (4, presentations[2]),
    ] {
        let id = fixture.app.state.board.live_thoughts()[index].id;
        fixture
            .app
            .state
            .board
            .thought_mut(id)
            .expect("long thought")
            .presentation = presentation;
    }
    fixture.app.state.focused_thought = fixture
        .app
        .state
        .board
        .live_thoughts()
        .first()
        .map(|thought| thought.id);
    scroll_combination_down(&mut fixture, presentations, area);
    scroll_combination_up(&mut fixture, presentations, area);
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        contents.iter().map(String::as_str).collect::<Vec<_>>()
    );
}

fn scroll_combination_down(
    fixture: &mut Fixture,
    presentations: [ThoughtPresentation; 3],
    area: Rect,
) {
    let mut previous = fixture.app.prepare_frame(area);
    for _ in 0..512 {
        fixture.pointer(4, 2, PointerKind::ScrollDown);
        let next = fixture.app.prepare_frame(area);
        assert_shared_geometry_step(&previous, &next, true, presentations);
        previous = next;
    }
    let bottom = fixture.app.prepare_frame(area);
    assert_eq!(
        bottom.insert.expect("insertion row").bottom(),
        bottom.board.bottom(),
        "bottom page must be filled for {presentations:?}: {bottom:#?}"
    );
}

fn scroll_combination_up(
    fixture: &mut Fixture,
    presentations: [ThoughtPresentation; 3],
    area: Rect,
) {
    let mut previous = fixture.app.prepare_frame(area);
    for _ in 0..512 {
        fixture.pointer(4, 2, PointerKind::ScrollUp);
        let next = fixture.app.prepare_frame(area);
        assert_shared_geometry_step(&previous, &next, false, presentations);
        previous = next;
    }
    let top = fixture.app.prepare_frame(area);
    assert_eq!((top.first_index, top.first_row_offset), (0, 0));
}

fn assert_shared_geometry_step(
    before: &proqi::ui::LayoutSnapshot,
    after: &proqi::ui::LayoutSnapshot,
    down: bool,
    presentations: [ThoughtPresentation; 3],
) {
    for thought in &before.thoughts {
        let Some(moved) = after.thought(thought.thought_id) else {
            continue;
        };
        let rows = if down {
            thought.area.y.saturating_sub(moved.area.y)
        } else {
            moved.area.y.saturating_sub(thought.area.y)
        };
        assert!(
            rows <= 1,
            "wheel input jumped shared thought for {presentations:?}: before={before:#?}; after={after:#?}"
        );
    }
}
