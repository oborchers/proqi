use ratatui_core::layout::Rect;

use super::{HitTarget, compute};
use crate::{
    application::{AppState, InteractionMode},
    domain::{
        Session, SessionBoard, SessionId, Thought, ThoughtId, ThoughtPosition, ThoughtPresentation,
        Timestamp,
    },
    ports::{
        editor::{EditorSnapshot, TextViewport},
        text_layout::wrap_rows,
    },
};

fn uuid_v7(seed: u8) -> uuid::Uuid {
    let mut bytes = [0; 16];
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = seed;
    uuid::Uuid::from_bytes(bytes)
}

fn empty_state() -> AppState {
    let now = Timestamp::from_millis(1);
    let session = Session::new(
        SessionId::from_uuid(uuid_v7(1)).expect("UUIDv7 session ID"),
        "/tmp".into(),
        now,
    )
    .expect("session");
    AppState::new(SessionBoard::new(session, Vec::new()).expect("board"))
}

fn long_state(presentation: ThoughtPresentation) -> (AppState, ThoughtId, String) {
    let now = Timestamp::from_millis(1);
    let session = Session::new(
        SessionId::from_uuid(uuid_v7(1)).expect("UUIDv7 session ID"),
        "/tmp".into(),
        now,
    )
    .expect("session");
    let thought_id = ThoughtId::from_uuid(uuid_v7(2)).expect("UUIDv7 thought ID");
    let content = (0..12)
        .map(|line| format!("long line {line} with enough words to wrap"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut thought = Thought::new(
        thought_id,
        session.id,
        content.clone(),
        ThoughtPosition::new(0),
        now,
    );
    thought.presentation = presentation;
    let mut state = AppState::new(SessionBoard::new(session, vec![thought]).expect("board"));
    state.focused_thought = Some(thought_id);
    (state, thought_id, content)
}

#[test]
fn empty_layout_exposes_shared_footer_and_insert_targets() {
    let layout = compute(&empty_state(), None, Rect::new(0, 0, 20, 5), 0, true, false);
    assert_eq!(layout.header, Rect::new(0, 0, 20, 0));
    let insert = layout.insert.expect("visible insertion row");
    assert_eq!(layout.hit_test(insert.x, insert.y), Some(HitTarget::Insert));
    assert!(
        layout
            .controls
            .iter()
            .any(|(target, _)| { matches!(target, HitTarget::Commands | HitTarget::Help) })
    );
}

#[test]
fn overlays_avoid_the_footer_or_deliberately_cover_the_full_shallow_frame() {
    let mut roomy = compute(
        &empty_state(),
        None,
        Rect::new(0, 0, 72, 12),
        0,
        true,
        false,
    );
    roomy.configure_overlay(3, 5);
    let overlay = roomy.overlay.expect("roomy overlay");
    assert!(overlay.area.bottom() <= roomy.footer.y);

    let mut shallow = compute(&empty_state(), None, Rect::new(0, 0, 72, 8), 0, true, false);
    shallow.configure_overlay(3, 5);
    let overlay = shallow.overlay.expect("shallow overlay");
    assert_eq!(overlay.area.x, shallow.area.x);
    assert_eq!(overlay.area.width, shallow.area.width);
    assert!(overlay.area.bottom() > shallow.footer.y);
}

#[test]
fn expanded_viewport_clipping_is_scrollable_without_an_expand_affordance() {
    let (state, _, _) = long_state(ThoughtPresentation::Expanded);
    let layout = compute(&state, None, Rect::new(0, 0, 32, 9), 0, false, false);
    let thought = &layout.thoughts[0];
    assert!(thought.viewport_clipped);
    assert!(thought.scrollable_hidden);
    assert_eq!(thought.hidden_rows, 0);
    assert!(thought.overflow.is_none());
}

#[test]
fn editing_ignores_a_stale_collapsed_cap_and_uses_the_available_viewport() {
    let (mut state, thought_id, content) = long_state(ThoughtPresentation::Collapsed);
    state.mode = InteractionMode::Edit { thought_id };
    let width = 30;
    let snapshot = EditorSnapshot {
        content: content.clone(),
        cursor: crate::domain::TextPosition::default(),
        selection: None,
        viewport: TextViewport::new(width, 1),
        scroll_row: 0,
        visual_lines: wrap_rows(&content, usize::from(width))
            .into_iter()
            .map(|row| row.visual)
            .collect(),
    };
    let layout = compute(
        &state,
        Some(&snapshot),
        Rect::new(0, 0, 32, 9),
        0,
        false,
        false,
    );
    let thought = &layout.thoughts[0];
    assert!(thought.area.height > 2);
    assert!(thought.viewport_clipped);
    assert_eq!(thought.hidden_rows, 0);
    assert!(thought.overflow.is_none());
}
