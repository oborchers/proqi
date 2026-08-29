use super::*;
use crate::domain::{Session, SessionBoard, SessionId, Thought, ThoughtPosition, Timestamp};
use crate::ports::{
    editor::{EditorSnapshot, TextViewport},
    text_layout::wrap_rows,
};

fn uuid_v7(seed: u8) -> uuid::Uuid {
    let mut bytes = [0; 16];
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = seed;
    uuid::Uuid::from_bytes(bytes)
}

fn state(contents: &[(&str, ThoughtPresentation)]) -> AppState {
    let now = Timestamp::from_millis(1);
    let session = Session::new(
        SessionId::from_uuid(uuid_v7(1)).expect("session ID"),
        "/tmp".into(),
        now,
    )
    .expect("session");
    let thoughts = contents
        .iter()
        .enumerate()
        .map(|(index, (content, presentation))| {
            let id = ThoughtId::from_uuid(uuid_v7(u8::try_from(index + 2).expect("seed")))
                .expect("thought ID");
            let mut thought = Thought::new(
                id,
                session.id,
                (*content).to_owned(),
                ThoughtPosition::new(u32::try_from(index).expect("position")),
                now,
            );
            thought.presentation = *presentation;
            thought
        })
        .collect();
    AppState::new(SessionBoard::new(session, thoughts).expect("board"))
}

#[test]
fn multiple_long_thought_presentation_matrix_scrolls_every_row_both_directions() {
    let long = [
        long_content("alpha"),
        long_content("beta"),
        long_content("gamma"),
    ];
    for presentations in presentation_combinations() {
        let state = state(&[
            (&long[0], presentations[0]),
            (
                "ordinary between alpha and beta",
                ThoughtPresentation::Automatic,
            ),
            (&long[1], presentations[1]),
            (
                "ordinary between beta and gamma",
                ThoughtPresentation::Automatic,
            ),
            (&long[2], presentations[2]),
        ]);
        for (width, height, density) in viewport_cases() {
            assert_flow_case(&state, presentations, width, height, density);
        }
    }
}

fn long_content(label: &str) -> String {
    (0..18)
        .map(|line| {
            format!("{label} {line:02} · Grüße 界 👩‍💻 · enough words to wrap at every tested width")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn presentation_combinations() -> Vec<[ThoughtPresentation; 3]> {
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

fn viewport_cases() -> Vec<(u16, u16, crate::ui::settings::BoardDensity)> {
    [18, 34, 72]
        .into_iter()
        .flat_map(|width| {
            [5, 11, 24].into_iter().flat_map(move |height| {
                [
                    crate::ui::settings::BoardDensity::Comfortable,
                    crate::ui::settings::BoardDensity::Compact,
                ]
                .into_iter()
                .map(move |density| (width, height, density))
            })
        })
        .collect()
}

fn assert_flow_case(
    state: &AppState,
    presentations: [ThoughtPresentation; 3],
    width: u16,
    height: u16,
    density: crate::ui::settings::BoardDensity,
) {
    let flow = BoardFlow::measure(state, None, width, height, density);
    assert_eq!(
        flow.thoughts
            .iter()
            .filter(|thought| thought.natural_rows > 2)
            .count(),
        3,
        "all presentation variants must exercise long thoughts"
    );
    let start = flow.resolve(
        BoardViewport::default(),
        state.focused_thought,
        false,
        height,
    );
    let bottom = scroll_to_bottom(&flow, start, presentations, width, height, density);
    assert_bottom_page(&flow, bottom, presentations, width, height, density);
    let top = scroll_to_top(&flow, bottom, height);
    assert_eq!(top.offset, 0);
    assert_eq!(top.geometry.current, start.geometry.current);
}

fn scroll_to_bottom(
    flow: &BoardFlow,
    mut current: ResolvedScroll,
    presentations: [ThoughtPresentation; 3],
    width: u16,
    height: u16,
    density: crate::ui::settings::BoardDensity,
) -> ResolvedScroll {
    let mut steps = 0;
    while let Some(next) = current.geometry.next {
        let advanced = flow.resolve(BoardViewport::Manual(next), None, false, height);
        assert_eq!(
            advanced.offset,
            current.offset + 1,
            "one down event must advance one row: {presentations:?}, {width}x{height}, {density:?}"
        );
        assert_eq!(advanced.geometry.previous, Some(current.geometry.current));
        current = advanced;
        steps += 1;
        assert!(steps <= flow.total_rows, "scroll flow must terminate");
    }
    current
}

fn assert_bottom_page(
    flow: &BoardFlow,
    bottom: ResolvedScroll,
    presentations: [ThoughtPresentation; 3],
    width: u16,
    height: u16,
    density: crate::ui::settings::BoardDensity,
) {
    assert_eq!(bottom.geometry.current, bottom.geometry.maximum);
    assert_eq!(bottom.geometry.next, None);
    let viewport_rows = usize::from(height.saturating_sub(flow.top_padding).max(1));
    let insert_row = flow.insert_row.expect("board insertion row");
    assert!(
        (bottom.offset..bottom.offset + viewport_rows).contains(&insert_row),
        "bottom page must expose insertion: {presentations:?}, {width}x{height}, {density:?}"
    );
}

fn scroll_to_top(flow: &BoardFlow, mut current: ResolvedScroll, height: u16) -> ResolvedScroll {
    while let Some(previous) = current.geometry.previous {
        let reversed = flow.resolve(BoardViewport::Manual(previous), None, false, height);
        assert_eq!(reversed.offset + 1, current.offset);
        assert_eq!(reversed.geometry.next, Some(current.geometry.current));
        current = reversed;
    }
    current
}

#[test]
fn every_visual_row_anchor_has_symmetric_neighbors() {
    let long = (0..8)
        .map(|line| format!("A界B {line} with Grüße and enough words to wrap"))
        .collect::<Vec<_>>()
        .join("\n");
    let state = state(&[
        ("before", ThoughtPresentation::Automatic),
        (&long, ThoughtPresentation::Expanded),
        ("between", ThoughtPresentation::Collapsed),
        (&long, ThoughtPresentation::Automatic),
        ("after", ThoughtPresentation::Automatic),
    ]);
    let flow = BoardFlow::measure(
        &state,
        None,
        24,
        10,
        crate::ui::settings::BoardDensity::Comfortable,
    );
    let mut current = flow.resolve(BoardViewport::default(), None, false, 10);
    let mut steps = 0;
    while let Some(next) = current.geometry.next {
        let advanced = flow.resolve(BoardViewport::Manual(next), None, false, 10);
        assert_eq!(advanced.geometry.previous, Some(current.geometry.current));
        assert_ne!(advanced.geometry.current, current.geometry.current);
        current = advanced;
        steps += 1;
        assert!(steps < 256, "scroll flow must terminate");
    }
    assert_eq!(current.geometry.current, current.geometry.maximum);

    while let Some(previous) = current.geometry.previous {
        let reversed = flow.resolve(BoardViewport::Manual(previous), None, false, 10);
        assert_eq!(reversed.geometry.next, Some(current.geometry.current));
        current = reversed;
    }
    assert_eq!(
        current.geometry.current,
        flow.resolve(BoardViewport::default(), None, false, 10)
            .geometry
            .current
    );
}

#[test]
fn active_editor_rows_replace_stale_durable_content_during_measurement() {
    let mut state = state(&[("durable", ThoughtPresentation::Collapsed)]);
    let thought_id = state.board.live_thoughts()[0].id;
    state.mode = InteractionMode::Edit { thought_id };
    let content = "edited one\nedited two\nedited three\nedited four".to_owned();
    let visual_lines = wrap_rows(&content, 20)
        .into_iter()
        .map(|row| row.visual)
        .collect::<Vec<_>>();
    let editor = EditorSnapshot {
        content,
        cursor: crate::domain::TextPosition::default(),
        selection: None,
        viewport: TextViewport::new(20, 8),
        scroll_row: 0,
        visual_lines,
    };

    let flow = BoardFlow::measure(
        &state,
        Some(&editor),
        20,
        8,
        crate::ui::settings::BoardDensity::Compact,
    );

    assert_eq!(flow.thoughts[0].natural_rows, 4);
    assert_eq!(flow.thoughts[0].content_rows, 4);
    assert_eq!(flow.thoughts[0].overflow_row, None);
}

#[test]
fn unicode_content_anchor_survives_width_reflow_by_projected_byte() {
    let content = "A界B e\u{301} 👩‍💻 wraps through several cells and remains anchored";
    let state = state(&[(content, ThoughtPresentation::Expanded)]);
    let narrow = BoardFlow::measure(
        &state,
        None,
        12,
        5,
        crate::ui::settings::BoardDensity::Compact,
    );
    let thought = &narrow.thoughts[0];
    let byte = thought.row_starts[2];
    let anchor = ScrollAnchor::Content {
        thought_id: thought.thought_id,
        byte,
    };
    let wide = BoardFlow::measure(
        &state,
        None,
        24,
        5,
        crate::ui::settings::BoardDensity::Compact,
    );
    let resolved = wide.resolve(BoardViewport::Manual(anchor), None, false, 5);
    let ScrollAnchor::Content {
        thought_id,
        byte: reflowed,
    } = resolved.geometry.current
    else {
        panic!("reflowed content anchor");
    };
    assert_eq!(thought_id, thought.thought_id);
    assert!(reflowed <= byte);
    let row = wide.thoughts[0]
        .row_starts
        .iter()
        .position(|start| *start == reflowed)
        .expect("reflowed row");
    assert!(
        wide.thoughts[0]
            .row_starts
            .get(row + 1)
            .is_none_or(|next| *next > byte)
    );
}
