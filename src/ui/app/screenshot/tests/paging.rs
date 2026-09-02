use super::behavior::app_with_thought;
use crate::{
    ports::{environment::IdGenerator as _, runtime::CaptureOwnerInfo},
    ui::{FastNavigation, UiInput, UiKey},
};

#[test]
fn takeover_fast_navigation_clamps_without_scrolling_the_board() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    let session_id = app.state.board.session.id;
    app.screenshot_conflict(owner(&mut ids, session_id));
    app.handle(
        UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Next,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.screenshot_takeover_view().expect("takeover").1, 1);
    app.handle(
        UiInput::Key(UiKey::FastNavigation {
            direction: FastNavigation::Previous,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.screenshot_takeover_view().expect("takeover").1, 0);
}

fn owner(
    ids: &mut crate::adapters::memory::FakeIdGenerator,
    session_id: crate::domain::SessionId,
) -> CaptureOwnerInfo {
    CaptureOwnerInfo {
        instance_id: ids.instance_id(),
        session_id,
        pid: 42,
        version: "test".to_owned(),
        capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
        control_protocol: crate::ports::control::CONTROL_PROTOCOL_VERSION,
        control_endpoint: "private-control-endpoint".to_owned(),
        started_at: crate::domain::Timestamp::from_millis(1),
    }
}
