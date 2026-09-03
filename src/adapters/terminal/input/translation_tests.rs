//! Modifier-preserving translation contracts for ordinary Space input.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    ports::editor::CursorMovement,
    ui::{UiInput, UiKey, VisualRowEdge},
};

use super::{
    translate,
    translation::{ModifierPlatform, translate_key_for_platform},
};

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

#[test]
fn macos_command_horizontal_arrows_use_wrapped_rows_with_and_without_shift() {
    for (code, edge) in [
        (KeyCode::Left, VisualRowEdge::Start),
        (KeyCode::Right, VisualRowEdge::End),
    ] {
        assert_eq!(
            translate_key_for_platform(
                KeyEvent::new(code, KeyModifiers::SUPER),
                ModifierPlatform::MacOs,
            ),
            Some(UiKey::MoveVisualRow { edge })
        );
        assert_eq!(
            translate_key_for_platform(
                KeyEvent::new(code, KeyModifiers::SUPER | KeyModifiers::SHIFT,),
                ModifierPlatform::MacOs,
            ),
            Some(UiKey::ExtendVisualRow { edge })
        );
    }
}

#[test]
fn non_macos_control_and_shift_control_horizontal_arrows_move_by_word() {
    for (code, movement) in [
        (KeyCode::Left, CursorMovement::WordBack),
        (KeyCode::Right, CursorMovement::WordForward),
    ] {
        for extend_selection in [false, true] {
            let modifiers = if extend_selection {
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            } else {
                KeyModifiers::CONTROL
            };
            assert_eq!(
                translate_key_for_platform(KeyEvent::new(code, modifiers), ModifierPlatform::Other,),
                Some(UiKey::Move {
                    movement,
                    extend_selection,
                })
            );
        }
    }
}

#[test]
fn macos_option_word_and_ordinary_shift_horizontal_arrows_keep_existing_meanings() {
    for (code, word, grapheme) in [
        (
            KeyCode::Left,
            CursorMovement::WordBack,
            CursorMovement::GraphemeBack,
        ),
        (
            KeyCode::Right,
            CursorMovement::WordForward,
            CursorMovement::GraphemeForward,
        ),
    ] {
        assert_eq!(
            translate_key_for_platform(
                KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT),
                ModifierPlatform::MacOs,
            ),
            Some(UiKey::Move {
                movement: word,
                extend_selection: true,
            })
        );
        assert_eq!(
            translate_key_for_platform(
                KeyEvent::new(code, KeyModifiers::SHIFT),
                ModifierPlatform::MacOs,
            ),
            Some(UiKey::Move {
                movement: grapheme,
                extend_selection: true,
            })
        );
    }
}
