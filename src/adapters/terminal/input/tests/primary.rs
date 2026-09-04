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
fn shifted_reserved_primary_chords_preserve_shift_and_uppercase_without_shift_stays_unshifted() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        for (lowercase, uppercase, expected) in [
            ('a', 'A', UiKey::SelectAll),
            ('c', 'C', UiKey::Copy),
            ('x', 'X', UiKey::Cut),
            ('d', 'D', UiKey::Duplicate),
            ('q', 'Q', UiKey::Quit),
            ('y', 'Y', UiKey::Redo),
        ] {
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(uppercase),
                    modifier,
                ))),
                Some(UiInput::Key(expected)),
                "uppercase {uppercase:?}, modifier {modifier:?}"
            );
            for character in [lowercase, uppercase] {
                assert_eq!(
                    translate(Event::Key(KeyEvent::new(
                        KeyCode::Char(character),
                        modifier | KeyModifiers::SHIFT,
                    ))),
                    Some(UiInput::Key(UiKey::PrimaryShiftCharacter(character))),
                    "shifted {character:?}, modifier {modifier:?}"
                );
            }
        }
        for character in ['v', 'V'] {
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    modifier
                ))),
                Some(UiInput::Key(UiKey::PasteClipboard))
            );
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    modifier | KeyModifiers::SHIFT,
                ))),
                Some(UiInput::Key(UiKey::PasteClipboardReflow))
            );
        }
    }
}

#[test]
fn primary_y_is_redo_only_without_distinct_shift() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        for character in ['y', 'Y'] {
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    modifier
                ))),
                Some(UiInput::Key(UiKey::Redo))
            );
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    modifier | KeyModifiers::SHIFT,
                ))),
                Some(UiInput::Key(UiKey::PrimaryShiftCharacter(character)))
            );
        }
    }
}

#[test]
fn uppercase_z_without_shift_is_undo_while_distinct_shift_is_redo() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Char('Z'), modifier))),
            Some(UiInput::Key(UiKey::Undo))
        );
        assert_eq!(
            translate(Event::Key(KeyEvent::new(
                KeyCode::Char('Z'),
                modifier | KeyModifiers::SHIFT,
            ))),
            Some(UiInput::Key(UiKey::Redo))
        );
    }
}
