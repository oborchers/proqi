//! Keybinding validation and normalized Board command contracts.

use super::{BoardCommand, KeyBindings};
use crate::ui::UiKey;

#[test]
fn board_submission_chords_resolve_to_the_configured_submission_commands() {
    let bindings = KeyBindings {
        submit_remove: '界',
        submit_keep: '語',
        ..KeyBindings::default()
    };
    assert!(bindings.validate().is_ok());
    for (key, command) in [
        (
            UiKey::Character(bindings.submit_remove),
            BoardCommand::SubmitRemove,
        ),
        (UiKey::Submit, BoardCommand::SubmitRemove),
        (
            UiKey::Character(bindings.submit_keep),
            BoardCommand::SubmitKeep,
        ),
        (UiKey::SubmitKeep, BoardCommand::SubmitKeep),
    ] {
        assert_eq!(bindings.command_for_key(key), Some(command));
    }
    assert_eq!(bindings.command_for_key(UiKey::Character('s')), None);
    assert_eq!(bindings.command_for_key(UiKey::Character('S')), None);
    assert_eq!(bindings.command_for_key(UiKey::Enter), None);
}

#[test]
fn ambiguous_bindings_are_rejected() {
    let mut bindings = KeyBindings::default();
    bindings.edit = bindings.new;
    assert!(bindings.validate().is_err());
}

#[test]
fn recovery_keys_cannot_be_used_for_quit() {
    for reserved in ['r', 'w'] {
        let bindings = KeyBindings {
            quit: reserved,
            ..KeyBindings::default()
        };
        assert!(bindings.validate().is_err());
    }
}

#[test]
fn established_board_binding_precedes_compatible_transform_collision() {
    let bindings = KeyBindings {
        new: 't',
        ..KeyBindings::default()
    };
    assert!(bindings.validate().is_ok());
    assert_eq!(bindings.command('t'), Some(BoardCommand::New));
}

#[test]
fn paste_reflow_has_a_configurable_board_fallback_without_breaking_old_collisions() {
    let defaults = KeyBindings::default();
    assert_eq!(defaults.command('p'), Some(BoardCommand::PasteReflow));
    let remapped: KeyBindings = toml::from_str("paste_reflow = 'g'").expect("remap");
    assert_eq!(remapped.command('g'), Some(BoardCommand::PasteReflow));

    let old_collision: KeyBindings = toml::from_str("new = 'p'").expect("old config");
    assert_eq!(old_collision.validate(), Ok(()));
    assert_eq!(old_collision.command('p'), Some(BoardCommand::New));
}

#[test]
fn reserved_primary_transform_bindings_are_rejected() {
    for reserved in ['a', 'c', 'd', 'n', 'p', 'q', 'u', 'v', 'x', 'y', 'z'] {
        let bindings = KeyBindings {
            transform: reserved,
            ..KeyBindings::default()
        };
        assert!(bindings.validate().is_err(), "reserved: {reserved}");
    }
}

#[test]
fn sentence_deletion_rejects_primary_chords_consumed_before_edit_dispatch() {
    for reserved in ['A', 'C', 'D', 'N', 'P', 'Q', 'V', 'X', 'Y', 'Z'] {
        let bindings = KeyBindings {
            delete_sentence: reserved,
            ..KeyBindings::default()
        };
        assert!(bindings.validate().is_err(), "reserved suffix {reserved}");
    }
}

#[test]
fn sentence_deletion_rejects_unreachable_shifted_suffixes() {
    for unreachable in ['g', '1', '!', 'Ü'] {
        let bindings = KeyBindings {
            delete_sentence: unreachable,
            ..KeyBindings::default()
        };
        assert!(
            bindings.validate().is_err(),
            "unreachable suffix {unreachable}"
        );
    }
}

#[test]
fn visual_row_fallbacks_reject_unreachable_reserved_and_duplicate_suffixes() {
    for unreachable in ['g', '1', 'A', 'Z', 'Ü'] {
        let bindings = KeyBindings {
            select_visual_row_start: unreachable,
            ..KeyBindings::default()
        };
        assert!(
            bindings.validate().is_err(),
            "unreachable suffix {unreachable}"
        );
    }
    let duplicate = KeyBindings {
        select_visual_row_end: 'H',
        ..KeyBindings::default()
    };
    assert!(duplicate.validate().is_err());
}

#[test]
fn visual_row_fallbacks_do_not_invalidate_existing_board_remaps() {
    let bindings = KeyBindings {
        new: 'H',
        focus_up: 'L',
        ..KeyBindings::default()
    };
    assert_eq!(bindings.validate(), Ok(()));
}
