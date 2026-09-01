//! Modifier-preserving translation contracts for ordinary Space input.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::ui::{UiInput, UiKey};

use super::translate;

#[test]
fn only_unmodified_space_receives_the_placeholder_aware_identity() {
    assert_eq!(
        translate(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        ))),
        Some(UiInput::Key(UiKey::UnmodifiedSpace))
    );
    for modifiers in [KeyModifiers::SHIFT, KeyModifiers::ALT] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Char(' '), modifiers))),
            Some(UiInput::Key(UiKey::Character(' '))),
            "modifiers: {modifiers:?}"
        );
    }
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Char(' '), modifiers))),
            Some(UiInput::Key(UiKey::PrimaryCharacter(' '))),
            "modifiers: {modifiers:?}"
        );
    }
}
