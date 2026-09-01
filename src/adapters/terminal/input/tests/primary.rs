//! Case and modifier parity for terminal Primary-character normalization.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::ui::{UiInput, UiKey};

use super::super::translate;

#[test]
fn command_and_meta_shortcuts_share_semantics() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), modifier));
        assert_eq!(translate(event), Some(UiInput::Key(UiKey::SelectAll)));
    }
}

#[test]
fn reserved_primary_chords_ignore_shifted_case_encoding() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        for (lowercase, uppercase, expected) in [
            ('a', 'A', UiKey::SelectAll),
            ('c', 'C', UiKey::Copy),
            ('x', 'X', UiKey::Cut),
            ('v', 'V', UiKey::PasteClipboard),
            ('d', 'D', UiKey::Duplicate),
            ('q', 'Q', UiKey::Quit),
            ('p', 'P', UiKey::PickerPrevious),
            ('n', 'N', UiKey::PickerNext),
            ('y', 'Y', UiKey::Redo),
        ] {
            for character in [lowercase, uppercase] {
                assert_eq!(
                    translate(Event::Key(KeyEvent::new(
                        KeyCode::Char(character),
                        modifier | KeyModifiers::SHIFT,
                    ))),
                    Some(UiInput::Key(expected)),
                    "character {character:?}, modifier {modifier:?}"
                );
            }
        }
    }
}
