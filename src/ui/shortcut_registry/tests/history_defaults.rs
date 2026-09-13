//! Terminal-safe macOS history defaults and their presentation contract.

use super::super::{KeymapDocument, ShortcutPlatform, ShortcutRegistry};
use super::stroke;
use crate::ui::{
    KeyBindings, KeyPhase, LogicalKey, LogicalModifiers as M, ShortcutActionId as A,
    ShortcutContext as C, ShortcutContextStack,
};

fn action(
    registry: &ShortcutRegistry,
    context: C,
    key: LogicalKey,
    modifiers: M,
    phase: KeyPhase,
) -> Option<A> {
    let mut input = stroke(key, modifiers);
    input.phase = phase;
    registry
        .dispatch(&ShortcutContextStack::new([C::Board, context]), input)
        .and_then(|resolved| resolved.action)
}

#[test]
fn macos_control_history_is_exact_for_every_owner_and_phase() {
    let registry =
        ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs).unwrap();
    let control_shift = M::CONTROL.union(M::SHIFT);
    for context in super::super::inventory::bindings::vocabulary::KEYBOARD_CONTEXTS {
        for phase in [KeyPhase::Press, KeyPhase::Repeat] {
            for (key, modifiers, expected) in [
                (LogicalKey::Character('z'), M::CONTROL, A::Undo),
                (LogicalKey::Character('z'), control_shift, A::Redo),
                (LogicalKey::Character('Z'), control_shift, A::Redo),
                (LogicalKey::Character('Z'), M::CONTROL, A::Redo),
                (LogicalKey::Character('y'), M::CONTROL, A::Redo),
                (LogicalKey::Character('Y'), M::CONTROL, A::Redo),
            ] {
                assert_eq!(
                    action(&registry, *context, key, modifiers, phase),
                    Some(expected),
                    "{context:?} {key:?} {modifiers:?} {phase:?}",
                );
            }
        }
        for (key, modifiers) in [
            (LogicalKey::Character('z'), M::CONTROL),
            (LogicalKey::Character('z'), control_shift),
            (LogicalKey::Character('y'), M::CONTROL),
        ] {
            assert_eq!(
                action(&registry, *context, key, modifiers, KeyPhase::Release),
                None,
            );
        }
    }
}

#[test]
fn macos_history_labels_prefer_control_and_retain_primary_and_board_u() {
    let registry =
        ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs).unwrap();
    for context in super::super::inventory::bindings::vocabulary::KEYBOARD_CONTEXTS {
        assert_eq!(registry.compact_help_label(*context, &[A::Undo]), "Ctrl+Z");
        assert_eq!(
            registry.compact_help_label(*context, &[A::Redo]),
            "Ctrl+Shift+Z"
        );
        let undo = registry.labels(*context, A::Undo);
        assert!(undo.contains(&"Ctrl+Z".to_owned()), "{context:?}");
        assert!(undo.contains(&"Cmd+Z".to_owned()), "{context:?}");
        let redo = registry.labels(*context, A::Redo);
        for expected in ["Ctrl+Shift+Z", "Ctrl+Y", "Cmd+Shift+Z", "Cmd+Y"] {
            assert!(
                redo.contains(&expected.to_owned()),
                "{context:?} {expected}"
            );
        }
    }
    assert!(registry.labels(C::Board, A::Undo).contains(&"u".to_owned()));
}

#[test]
fn macos_history_configuration_replaces_only_the_selected_owner_and_action() {
    let document: KeymapDocument = toml::from_str(
        "schema_version=1\n[macos.search]\n\"history.undo\"=[{key='F34'}]\n\"history.redo\"=[{key='F35'}]",
    )
    .unwrap();
    let registry = document.resolve(ShortcutPlatform::MacOs).unwrap();
    let control_shift = M::CONTROL.union(M::SHIFT);
    for (key, modifiers) in [
        (LogicalKey::Character('z'), M::CONTROL),
        (LogicalKey::Character('z'), control_shift),
        (LogicalKey::Character('y'), M::CONTROL),
        (LogicalKey::Character('z'), M::SUPER),
        (LogicalKey::Character('y'), M::META),
    ] {
        assert_eq!(
            action(&registry, C::Search, key, modifiers, KeyPhase::Press),
            None,
        );
    }
    assert_eq!(
        action(
            &registry,
            C::Search,
            LogicalKey::Function(34),
            M::NONE,
            KeyPhase::Press,
        ),
        Some(A::Undo),
    );
    assert_eq!(
        action(
            &registry,
            C::Search,
            LogicalKey::Function(35),
            M::NONE,
            KeyPhase::Press,
        ),
        Some(A::Redo),
    );
    assert_eq!(
        action(
            &registry,
            C::Edit,
            LogicalKey::Character('z'),
            M::CONTROL,
            KeyPhase::Press,
        ),
        Some(A::Undo),
    );
}

#[test]
fn macos_raw_control_remains_action_specific() {
    let registry =
        ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs).unwrap();
    for context in super::super::inventory::bindings::vocabulary::KEYBOARD_CONTEXTS {
        for character in ['a', 'c', 'd', 'q', 'v', 'x'] {
            assert_eq!(
                action(
                    &registry,
                    *context,
                    LogicalKey::Character(character),
                    M::CONTROL,
                    KeyPhase::Press,
                ),
                None,
                "{context:?} {character}",
            );
        }
        for extra in [M::ALT, M::SUPER, M::META, M::HYPER] {
            assert_eq!(
                action(
                    &registry,
                    *context,
                    LogicalKey::Character('z'),
                    M::CONTROL.union(extra),
                    KeyPhase::Press,
                ),
                None,
                "{context:?} {extra:?}",
            );
        }
    }
}
