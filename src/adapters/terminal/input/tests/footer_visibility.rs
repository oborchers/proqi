//! Footer visibility is a plain Board-mode shortcut.

use super::*;

#[test]
fn footer_toggle_accepts_an_unmodified_lowercase_h_report() {
    assert_eq!(
        translate(Event::Key(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        ))),
        Some(UiInput::Key(UiKey::Shortcut(
            crate::ui::ShortcutActionId::ToggleFooter,
        ))),
    );
}
