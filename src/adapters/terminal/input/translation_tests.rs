//! Modifier-preserving translation contracts for ordinary Space input.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::{
    ports::editor::CursorMovement,
    ui::{UiInput, UiKey, VisualRowEdge},
};

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

#[test]
fn platform_primary_shift_horizontal_arrows_are_visual_row_selection() {
    let primary = if cfg!(target_os = "macos") {
        KeyModifiers::SUPER
    } else {
        KeyModifiers::CONTROL
    };
    for (code, edge) in [
        (KeyCode::Left, VisualRowEdge::Start),
        (KeyCode::Right, VisualRowEdge::End),
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(
                code,
                primary | KeyModifiers::SHIFT,
            ))),
            Some(UiInput::Key(UiKey::ExtendVisualRow { edge }))
        );
    }
}

#[test]
fn unshifted_primary_and_ordinary_shift_horizontal_arrows_keep_existing_meanings() {
    let primary = if cfg!(target_os = "macos") {
        KeyModifiers::SUPER
    } else {
        KeyModifiers::CONTROL
    };
    for (code, macos, portable, shifted) in [
        (
            KeyCode::Left,
            CursorMovement::GraphemeBack,
            CursorMovement::WordBack,
            CursorMovement::GraphemeBack,
        ),
        (
            KeyCode::Right,
            CursorMovement::GraphemeForward,
            CursorMovement::WordForward,
            CursorMovement::GraphemeForward,
        ),
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(code, primary))),
            Some(UiInput::Key(UiKey::Move {
                movement: if cfg!(target_os = "macos") {
                    macos
                } else {
                    portable
                },
                extend_selection: false,
            }))
        );
        assert_eq!(
            translate(Event::Key(KeyEvent::new(code, KeyModifiers::SHIFT))),
            Some(UiInput::Key(UiKey::Move {
                movement: shifted,
                extend_selection: true,
            }))
        );
    }
}
