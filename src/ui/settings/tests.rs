//! Keybinding validation and normalized Board command contracts.

use super::{BoardDensity, KeyBindings};
use crate::ui::shortcut_registry::{ShortcutPlatform, ShortcutRegistry};
use crate::ui::{
    KeyStroke, LogicalKey, LogicalModifiers, ShortcutActionId as Action, ShortcutContext,
    ShortcutContextStack,
};

fn board_action(bindings: &KeyBindings, stroke: KeyStroke) -> Option<Action> {
    ShortcutRegistry::resolve(bindings, ShortcutPlatform::Portable)
        .expect("valid registry")
        .dispatch(&ShortcutContextStack::new([ShortcutContext::Board]), stroke)
        .and_then(|resolved| resolved.action)
}

fn board_character(bindings: &KeyBindings, character: char) -> Option<Action> {
    board_action(bindings, KeyStroke::press(LogicalKey::Character(character)))
}

fn primary(key: LogicalKey, shifted: bool) -> KeyStroke {
    let modifiers = if shifted {
        LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT)
    } else {
        LogicalModifiers::CONTROL
    };
    KeyStroke::press(key).with_modifiers(modifiers)
}

fn effective_fallbacks(bindings: &KeyBindings, character: char, action: Action) -> Vec<char> {
    (board_character(bindings, character) == Some(action))
        .then_some(character)
        .into_iter()
        .collect()
}

#[test]
fn board_submission_chords_resolve_to_the_configured_submission_commands() {
    let bindings = KeyBindings {
        submit_remove: '界',
        submit_keep: '語',
        ..KeyBindings::default()
    };
    assert!(bindings.validate().is_ok());
    for (stroke, action) in [
        (
            KeyStroke::press(LogicalKey::Character(bindings.submit_remove)),
            Action::SubmitRemove,
        ),
        (primary(LogicalKey::Enter, false), Action::SubmitRemove),
        (
            KeyStroke::press(LogicalKey::Character(bindings.submit_keep)),
            Action::SubmitKeep,
        ),
        (primary(LogicalKey::Enter, true), Action::SubmitKeep),
    ] {
        assert_eq!(board_action(&bindings, stroke), Some(action));
    }
    assert_eq!(board_character(&bindings, 's'), None);
    assert_eq!(board_character(&bindings, 'S'), None);
    assert_eq!(
        board_action(&bindings, KeyStroke::press(LogicalKey::Enter)),
        Some(Action::Edit)
    );
}

#[test]
fn ambiguous_bindings_are_rejected() {
    let mut bindings = KeyBindings::default();
    bindings.edit = bindings.new;
    assert!(bindings.validate().is_err());
}

#[test]
fn recovery_keys_cannot_be_used_for_quit() {
    for action in [
        super::super::ShortcutActionId::RetryStorage,
        super::super::ShortcutActionId::ExportRecovery,
    ] {
        let reserved = super::super::shortcut_registry::fixed_character_binding(
            action,
            super::super::ShortcutContext::Recovery,
        )
        .expect("recovery action has a fixed registry binding");
        let bindings = KeyBindings {
            quit: reserved,
            ..KeyBindings::default()
        };
        assert_eq!(
            bindings.validate(),
            Err("the quit binding cannot use the reserved recovery keys r or w")
        );
    }
}

#[test]
fn established_board_binding_precedes_compatible_transform_collision() {
    let bindings = KeyBindings {
        new: 't',
        ..KeyBindings::default()
    };
    assert!(bindings.validate().is_ok());
    assert_eq!(board_character(&bindings, 't'), Some(Action::New));
}

#[test]
fn paste_pair_has_configurable_board_fallbacks_without_breaking_old_collisions() {
    let defaults = KeyBindings::default();
    assert_eq!(board_character(&defaults, 'p'), Some(Action::PasteExact));
    assert_eq!(board_character(&defaults, 'P'), Some(Action::PasteReflow));
    assert_eq!(
        effective_fallbacks(&defaults, 'p', Action::PasteExact),
        vec!['p']
    );
    assert_eq!(
        effective_fallbacks(&defaults, 'P', Action::PasteReflow),
        vec!['P']
    );
    let remapped: KeyBindings = toml::from_str("paste = 'g'").expect("remap");
    assert_eq!(board_character(&remapped, 'g'), Some(Action::PasteExact));
    assert_eq!(board_character(&remapped, 'G'), Some(Action::PasteReflow));
    assert_eq!(
        effective_fallbacks(&remapped, 'g', Action::PasteExact),
        vec!['g']
    );
    assert_eq!(
        effective_fallbacks(&remapped, 'G', Action::PasteReflow),
        vec!['G']
    );

    let old_collision: KeyBindings = toml::from_str("new = 'p'").expect("old config");
    assert_eq!(old_collision.validate(), Ok(()));
    assert_eq!(board_character(&old_collision, 'p'), Some(Action::New));
    assert_eq!(
        board_character(&old_collision, 'P'),
        Some(Action::PasteReflow)
    );
    assert!(effective_fallbacks(&old_collision, 'p', Action::PasteExact).is_empty());
    assert_eq!(
        effective_fallbacks(&old_collision, 'P', Action::PasteReflow),
        vec!['P']
    );
}

#[test]
fn explicit_opposite_case_binding_precedes_the_reflow_fallback() {
    let bindings: KeyBindings =
        toml::from_str("paste = 'g'\nsubmit_keep = 'G'").expect("collision");
    assert_eq!(bindings.validate(), Ok(()));
    assert_eq!(board_character(&bindings, 'g'), Some(Action::PasteExact));
    assert_eq!(board_character(&bindings, 'G'), Some(Action::SubmitKeep));
    assert_eq!(
        effective_fallbacks(&bindings, 'g', Action::PasteExact),
        vec!['g']
    );
    assert!(effective_fallbacks(&bindings, 'G', Action::PasteReflow).is_empty());
}

#[test]
fn paste_pair_requires_a_lowercase_ascii_base_key() {
    for invalid in ['P', '1', '界', '\n'] {
        let bindings = KeyBindings {
            paste: invalid,
            ..KeyBindings::default()
        };
        assert!(
            bindings.validate().is_err(),
            "invalid paste key {invalid:?}"
        );
    }
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

#[test]
fn board_density_has_one_content_independent_responsive_breakpoint() {
    assert_eq!(BoardDensity::Comfortable.resolve(4), BoardDensity::Compact);
    assert_eq!(
        BoardDensity::Comfortable.resolve(5),
        BoardDensity::Comfortable
    );
    assert_eq!(BoardDensity::Compact.resolve(4), BoardDensity::Compact);
    assert_eq!(
        BoardDensity::Compact.resolve(u16::MAX),
        BoardDensity::Compact
    );
}
