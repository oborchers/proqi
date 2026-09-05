//! Lossless Crossterm-to-logical-key translation contracts.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use crate::ui::{KeyPhase, KeyStroke, LogicalKey, LogicalKeyState, LogicalModifiers, UiInput};

use super::{translate, translation::decode_key};

#[test]
fn ordinary_space_remains_a_neutral_character_at_the_terminal_boundary() {
    assert_eq!(
        translate(Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        ))),
        Some(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
            ' '
        ))))
    );
}

#[test]
fn all_logical_modifiers_remain_individually_expressible() {
    let modifiers = KeyModifiers::SHIFT
        | KeyModifiers::CONTROL
        | KeyModifiers::ALT
        | KeyModifiers::SUPER
        | KeyModifiers::META
        | KeyModifiers::HYPER;
    let decoded = decode_key(KeyEvent::new(KeyCode::Char('x'), modifiers));
    for modifier in [
        LogicalModifiers::SHIFT,
        LogicalModifiers::CONTROL,
        LogicalModifiers::ALT,
        LogicalModifiers::SUPER,
        LogicalModifiers::META,
        LogicalModifiers::HYPER,
    ] {
        assert!(decoded.modifiers.contains(modifier), "missing {modifier:?}");
    }
}

#[test]
fn repeat_phase_and_enhanced_state_survive_decoding() {
    let event = KeyEvent::new_with_kind_and_state(
        KeyCode::Enter,
        KeyModifiers::ALT,
        KeyEventKind::Repeat,
        KeyEventState::KEYPAD | KeyEventState::CAPS_LOCK | KeyEventState::NUM_LOCK,
    );
    let decoded = decode_key(event);
    assert_eq!(decoded.phase, KeyPhase::Repeat);
    assert_eq!(
        decoded.state,
        LogicalKeyState::KEYPAD
            .union(LogicalKeyState::CAPS_LOCK)
            .union(LogicalKeyState::NUM_LOCK)
    );
}

#[test]
fn release_events_can_be_decoded_but_are_not_delivered_as_input() {
    let event = KeyEvent::new_with_kind(
        KeyCode::Char('r'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert_eq!(decode_key(event).phase, KeyPhase::Release);
    assert_eq!(translate(Event::Key(event)), None);
}
