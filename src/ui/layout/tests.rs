use std::collections::BTreeSet;

use ratatui_core::layout::Rect;

use super::{HitTarget, compute};
use crate::{
    application::AppState,
    domain::{Session, SessionBoard, SessionId, Timestamp},
};

fn empty_state() -> AppState {
    let now = Timestamp::from_millis(1);
    let session = Session::new(
        SessionId::from_uuid(uuid::Uuid::now_v7()).expect("UUIDv7 session ID"),
        "/tmp".into(),
        now,
    )
    .expect("session");
    AppState::new(SessionBoard::new(session, Vec::new()).expect("board"))
}

#[test]
fn empty_layout_exposes_shared_footer_and_insert_targets() {
    let layout = compute(
        &empty_state(),
        None,
        Rect::new(0, 0, 20, 5),
        0,
        &BTreeSet::new(),
        true,
        false,
    );
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
        &BTreeSet::new(),
        true,
        false,
    );
    roomy.configure_overlay(3, 5);
    let overlay = roomy.overlay.expect("roomy overlay");
    assert!(overlay.area.bottom() <= roomy.footer.y);

    let mut shallow = compute(
        &empty_state(),
        None,
        Rect::new(0, 0, 72, 8),
        0,
        &BTreeSet::new(),
        true,
        false,
    );
    shallow.configure_overlay(3, 5);
    let overlay = shallow.overlay.expect("shallow overlay");
    assert_eq!(overlay.area.x, shallow.area.x);
    assert_eq!(overlay.area.width, shallow.area.width);
    assert!(overlay.area.bottom() > shallow.footer.y);
}
