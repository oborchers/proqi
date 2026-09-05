//! Primary remains policy, never a terminal-adapter modifier collapse.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ui::{LogicalKey, LogicalModifiers};

use super::super::translation::decode_key;

#[test]
fn terminal_decoding_preserves_each_possible_primary_source() {
    for (source, expected) in [
        (KeyModifiers::CONTROL, LogicalModifiers::CONTROL),
        (KeyModifiers::SUPER, LogicalModifiers::SUPER),
        (KeyModifiers::META, LogicalModifiers::META),
    ] {
        let stroke = decode_key(KeyEvent::new(KeyCode::Char('a'), source));
        assert_eq!(stroke.key, LogicalKey::Character('a'));
        assert_eq!(stroke.modifiers, expected);
    }
}

#[test]
fn raw_control_super_and_meta_never_collapse_together() {
    let stroke = decode_key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META,
    ));
    assert!(stroke.modifiers.contains(LogicalModifiers::CONTROL));
    assert!(stroke.modifiers.contains(LogicalModifiers::SUPER));
    assert!(stroke.modifiers.contains(LogicalModifiers::META));
}
