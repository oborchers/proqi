//! Exact submission defaults, presentation, configuration, and owner isolation.

use super::super::{KeymapDocument, ShortcutPlatform, ShortcutRegistry};
use super::stroke;
use crate::ui::{
    KeyBindings, KeyPhase, LogicalKey, LogicalModifiers as M, ShortcutActionId as A,
    ShortcutContext as C, ShortcutContextStack,
};

const CONTEXTS: [C; 5] = [
    C::Board,
    C::Compose,
    C::Edit,
    C::Invocation,
    C::InsertionBoundary,
];

#[test]
fn submission_aliases_are_exact_in_every_factory_context_and_phase() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry = ShortcutRegistry::resolve(&KeyBindings::default(), platform).unwrap();
        for context in CONTEXTS {
            assert_submission_phases(&registry, platform, context);
        }
    }
}

fn assert_submission_phases(registry: &ShortcutRegistry, platform: ShortcutPlatform, context: C) {
    for (shift, action) in [(M::NONE, A::SubmitRemove), (M::SHIFT, A::SubmitKeep)] {
        for modifier in [M::CONTROL, M::SUPER, M::META] {
            for phase in [KeyPhase::Press, KeyPhase::Repeat, KeyPhase::Release] {
                let mut key = stroke(LogicalKey::Enter, modifier.union(shift));
                key.phase = phase;
                let event = registry.inspect(&ShortcutContextStack::new([C::Board, context]), key);
                let expected = phase != KeyPhase::Release
                    && (platform == ShortcutPlatform::MacOs || modifier == M::CONTROL);
                assert_eq!(
                    event.action,
                    expected.then_some(action),
                    "{platform:?} {context:?} {key:?}"
                );
            }
        }
        for extra in [M::ALT, M::SUPER, M::META, M::HYPER] {
            let event = registry.inspect(
                &ShortcutContextStack::new([context]),
                stroke(LogicalKey::Enter, M::CONTROL.union(shift).union(extra)),
            );
            assert_eq!(event.action, None);
        }
    }
}

#[test]
fn macos_control_submission_is_not_primary_and_overlays_keep_ownership() {
    let registry =
        ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs).unwrap();
    for context in CONTEXTS {
        for character in ['a', 'c', 'v', 'x', 'z', 'q', 'd'] {
            let event = registry.inspect(
                &ShortcutContextStack::new([context]),
                stroke(LogicalKey::Character(character), M::CONTROL),
            );
            assert_eq!(event.action, None, "{context:?} {character}");
        }
    }
    for context in super::super::inventory::bindings::vocabulary::KEYBOARD_CONTEXTS {
        for (shift, action) in [(M::NONE, A::SubmitRemove), (M::SHIFT, A::SubmitKeep)] {
            let event = registry.inspect(
                &ShortcutContextStack::new([C::Edit, *context]),
                stroke(LogicalKey::Enter, M::CONTROL.union(shift)),
            );
            assert_eq!(
                event.action == Some(action),
                CONTEXTS.contains(context),
                "{context:?}"
            );
        }
        let event = registry.inspect(
            &ShortcutContextStack::new([C::Edit, *context]),
            stroke(LogicalKey::Escape, M::NONE),
        );
        assert_eq!(event.action, Some(A::Close), "{context:?}");
    }
}

#[test]
fn macos_submission_labels_prefer_control_and_retain_compatibility() {
    let registry =
        ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs).unwrap();
    for context in CONTEXTS {
        for (action, preferred, compatible) in [
            (A::SubmitRemove, "Ctrl+Enter", "Cmd+Enter"),
            (A::SubmitKeep, "Ctrl+Shift+Enter", "Cmd+Shift+Enter"),
        ] {
            assert_eq!(registry.compact_help_label(context, &[action]), preferred);
            let labels = registry.labels(context, action);
            assert!(labels.contains(&preferred.to_owned()));
            assert!(labels.contains(&compatible.to_owned()));
        }
    }
}

#[test]
fn submission_pair_replacement_disable_inheritance_and_collisions_are_exact() {
    for context in CONTEXTS {
        let name = context.configuration_id();
        let document: KeymapDocument = toml::from_str(&format!(
            "schema_version=1\n[macos.{name}]\n\"submission.submit_remove\"=[{{key='F35'}}]\n\"submission.submit_keep\"=[]"
        )).unwrap();
        let registry = document.resolve(ShortcutPlatform::MacOs).unwrap();
        for modifier in [M::CONTROL, M::SUPER, M::META] {
            for shift in [M::NONE, M::SHIFT] {
                let event = registry.inspect(
                    &ShortcutContextStack::new([context]),
                    stroke(LogicalKey::Enter, modifier.union(shift)),
                );
                assert_eq!(event.action, None);
            }
        }
        assert_eq!(
            registry
                .inspect(
                    &ShortcutContextStack::new([context]),
                    stroke(LogicalKey::Function(35), M::NONE)
                )
                .action,
            Some(A::SubmitRemove)
        );
        for other in CONTEXTS.into_iter().filter(|other| *other != context) {
            assert_eq!(
                registry
                    .inspect(
                        &ShortcutContextStack::new([other]),
                        stroke(LogicalKey::Enter, M::CONTROL)
                    )
                    .action,
                Some(A::SubmitRemove)
            );
        }
        let collision: KeymapDocument = toml::from_str(&format!(
            "schema_version=1\n[macos.{name}]\n\"submission.submit_keep\"=[{{key='Enter',modifiers=['Control']}}]"
        )).unwrap();
        assert!(collision.resolve(ShortcutPlatform::MacOs).is_err());
    }
}
