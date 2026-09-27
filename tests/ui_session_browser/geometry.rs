//! Session Browser hit geometry at degenerate viewport boundaries.

use super::*;

#[test]
fn zero_height_browser_has_no_phantom_footer_target() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let entry = item(
        &mut ids,
        Some("Zero height"),
        test_path("zero-height"),
        "hidden",
        10,
        resumable,
    );
    let mut browser = SessionBrowser::new(vec![entry], Timestamp::from_millis(20));
    let empty = browser.prepare_frame(ratatui_core::layout::Rect::new(0, 0, 44, 0));
    assert_eq!(empty.footer.height, 0);
    assert_eq!(
        browser.handle(UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        })),
        BrowserAction::Continue,
    );
}
