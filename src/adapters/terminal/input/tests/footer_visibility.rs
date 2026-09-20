//! Footer visibility accepts both terminal reports for Ctrl+Shift+H.

use super::*;

#[test]
fn footer_toggle_accepts_lowercase_and_uppercase_shifted_control_reports() {
    for character in ['h', 'H'] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ))),
            Some(UiInput::Key(UiKey::Shortcut(
                crate::ui::ShortcutActionId::ToggleFooter,
            ))),
        );
    }
}
