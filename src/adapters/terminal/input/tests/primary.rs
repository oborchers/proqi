//! Platform, case, and modifier parity for Primary-character normalization.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ui::UiKey;

use super::super::translation::{ModifierPlatform, translate_key_for_platform};

fn translated(platform: ModifierPlatform, code: KeyCode, modifiers: KeyModifiers) -> Option<UiKey> {
    translate_key_for_platform(KeyEvent::new(code, modifiers), platform)
}

fn primary_spellings() -> [(ModifierPlatform, KeyModifiers); 3] {
    [
        (ModifierPlatform::MacOs, KeyModifiers::SUPER),
        (ModifierPlatform::MacOs, KeyModifiers::META),
        (ModifierPlatform::Other, KeyModifiers::CONTROL),
    ]
}

#[test]
fn each_platform_accepts_only_its_primary_modifier() {
    for (platform, accepted, rejected) in [
        (
            ModifierPlatform::MacOs,
            [KeyModifiers::SUPER, KeyModifiers::META].as_slice(),
            [KeyModifiers::CONTROL].as_slice(),
        ),
        (
            ModifierPlatform::Other,
            [KeyModifiers::CONTROL].as_slice(),
            [KeyModifiers::SUPER, KeyModifiers::META].as_slice(),
        ),
    ] {
        for modifier in accepted {
            assert_eq!(
                translated(platform, KeyCode::Char('a'), *modifier),
                Some(UiKey::SelectAll)
            );
        }
        for modifier in rejected {
            assert_eq!(
                translated(platform, KeyCode::Char('a'), *modifier),
                None,
                "platform {platform:?}, modifier {modifier:?}"
            );
            assert_eq!(
                translated(platform, KeyCode::Enter, *modifier),
                None,
                "platform {platform:?}, modifier {modifier:?}"
            );
        }
    }
}

#[test]
fn mixed_command_modifiers_never_collapse_to_a_primary_command_or_text() {
    for (platform, modifiers) in [
        (
            ModifierPlatform::MacOs,
            KeyModifiers::SUPER | KeyModifiers::CONTROL,
        ),
        (
            ModifierPlatform::MacOs,
            KeyModifiers::META | KeyModifiers::ALT,
        ),
        (
            ModifierPlatform::MacOs,
            KeyModifiers::SUPER | KeyModifiers::META,
        ),
        (
            ModifierPlatform::Other,
            KeyModifiers::CONTROL | KeyModifiers::SUPER,
        ),
        (
            ModifierPlatform::Other,
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ),
    ] {
        for code in [KeyCode::Char('v'), KeyCode::Char('b'), KeyCode::Enter] {
            assert_eq!(
                translated(platform, code, modifiers),
                None,
                "platform {platform:?}, modifiers {modifiers:?}, code {code:?}"
            );
        }
    }
}

#[test]
fn shifted_reserved_primary_chords_preserve_shift_and_uppercase_without_shift_stays_unshifted() {
    for (platform, modifier) in primary_spellings() {
        for (lowercase, uppercase, expected) in [
            ('a', 'A', UiKey::SelectAll),
            ('c', 'C', UiKey::Copy),
            ('x', 'X', UiKey::Cut),
            ('d', 'D', UiKey::Duplicate),
            ('q', 'Q', UiKey::Quit),
            ('y', 'Y', UiKey::Redo),
        ] {
            assert_eq!(
                translated(platform, KeyCode::Char(uppercase), modifier),
                Some(expected),
                "uppercase {uppercase:?}, platform {platform:?}"
            );
            for character in [lowercase, uppercase] {
                assert_eq!(
                    translated(
                        platform,
                        KeyCode::Char(character),
                        modifier | KeyModifiers::SHIFT,
                    ),
                    Some(UiKey::PrimaryShiftCharacter(character)),
                    "shifted {character:?}, platform {platform:?}"
                );
            }
        }
    }
}

#[test]
fn primary_v_is_exact_and_distinct_shift_is_reflow() {
    for (platform, modifier) in primary_spellings() {
        for character in ['v', 'V'] {
            assert_eq!(
                translated(platform, KeyCode::Char(character), modifier),
                Some(UiKey::PasteClipboard)
            );
            assert_eq!(
                translated(
                    platform,
                    KeyCode::Char(character),
                    modifier | KeyModifiers::SHIFT,
                ),
                Some(UiKey::PasteClipboardReflow)
            );
        }
    }
}

#[test]
fn primary_y_and_z_keep_terminal_case_compatibility() {
    for (platform, modifier) in primary_spellings() {
        for character in ['y', 'Y'] {
            assert_eq!(
                translated(platform, KeyCode::Char(character), modifier),
                Some(UiKey::Redo)
            );
            assert_eq!(
                translated(
                    platform,
                    KeyCode::Char(character),
                    modifier | KeyModifiers::SHIFT,
                ),
                Some(UiKey::PrimaryShiftCharacter(character))
            );
        }
        assert_eq!(
            translated(platform, KeyCode::Char('Z'), modifier),
            Some(UiKey::Undo)
        );
        assert_eq!(
            translated(platform, KeyCode::Char('Z'), modifier | KeyModifiers::SHIFT,),
            Some(UiKey::Redo)
        );
    }
}
