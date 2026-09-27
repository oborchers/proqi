//! Reviewable Browser hover states across responsive themes.

use super::*;

#[test]
fn footer_has_a_complete_dark_wide_buffer() {
    let mut browser = snapshot_browser();
    let initial = draw_theme(&mut browser, 100, 20, ThemePreference::Dark);
    let footer = text(initial.backend().buffer())
        .lines()
        .last()
        .expect("footer")
        .to_owned();
    let column = u16::try_from(
        footer
            .find("Rename")
            .expect("Rename control in wide footer"),
    )
    .expect("terminal column");
    browser.handle(UiInput::Pointer(PointerInput {
        column,
        row: 19,
        kind: PointerKind::Move,
        extend_selection: false,
    }));

    let terminal = draw_theme(&mut browser, 100, 20, ThemePreference::Dark);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}

#[test]
fn entry_has_a_complete_light_narrow_buffer() {
    let mut browser = snapshot_browser();
    browser.handle(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: false,
    }));
    let _initial = draw_theme(&mut browser, 44, 10, ThemePreference::Light);
    browser.handle(UiInput::Pointer(PointerInput {
        column: 1,
        row: 6,
        kind: PointerKind::Move,
        extend_selection: false,
    }));

    let terminal = draw_theme(&mut browser, 44, 10, ThemePreference::Light);
    insta::assert_snapshot!(snapshot_buffer(terminal.backend().buffer()));
}
