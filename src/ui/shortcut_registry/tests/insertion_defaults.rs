//! Platform-specific relative insertion defaults and text-entry reservations.

use crate::ui::{
    KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack, UiKey,
};

use super::super::{ShortcutPlatform, ShortcutRegistry};
use super::{dispatch::dispatched, stroke};

const CONTROL_SHIFT: LogicalModifiers = LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT);

#[test]
fn macos_relative_insertion_defaults_are_truthful_and_match_new() {
    let macos = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid macOS registry");
    for context in [Context::Board, Context::InsertionBoundary] {
        for (key, modifiers, expected) in [
            (
                LogicalKey::Character('n'),
                LogicalModifiers::CONTROL,
                Action::InsertBelow,
            ),
            (
                LogicalKey::Character('n'),
                CONTROL_SHIFT,
                Action::InsertAbove,
            ),
            (
                LogicalKey::Character('N'),
                LogicalModifiers::CONTROL,
                Action::InsertAbove,
            ),
            (
                LogicalKey::Character('N'),
                CONTROL_SHIFT,
                Action::InsertAbove,
            ),
        ] {
            assert_eq!(
                dispatched(&macos, context, key, modifiers).action,
                Some(expected),
                "macOS {context:?}, {key:?}, {modifiers:?}",
            );
        }
        assert_eq!(
            dispatched(
                &macos,
                context,
                LogicalKey::Character('n'),
                LogicalModifiers::NONE,
            )
            .action,
            Some(Action::New),
        );
        for (key, expected) in [
            (LogicalKey::Up, Action::FocusPrevious),
            (LogicalKey::Down, Action::FocusNext),
        ] {
            assert_eq!(
                dispatched(&macos, context, key, LogicalModifiers::ALT).action,
                Some(expected),
            );
        }
    }
    assert_eq!(
        macos.compact_help_label(Context::Board, &[Action::InsertAbove]),
        "Ctrl+Shift+N",
    );
    assert_eq!(
        macos.compact_help_label(Context::Board, &[Action::InsertBelow]),
        "Ctrl+N",
    );
}

#[test]
fn portable_relative_insertion_defaults_remain_directional() {
    let remapped = KeyBindings {
        focus_up: 'b',
        focus_down: 'g',
        ..KeyBindings::default()
    };
    let portable = ShortcutRegistry::resolve(&remapped, ShortcutPlatform::Portable)
        .expect("valid portable registry");
    for context in [Context::Board, Context::InsertionBoundary] {
        for (key, expected) in [
            (LogicalKey::Up, Action::InsertAbove),
            (LogicalKey::Character('b'), Action::InsertAbove),
            (LogicalKey::Down, Action::InsertBelow),
            (LogicalKey::Character('g'), Action::InsertBelow),
        ] {
            assert_eq!(
                dispatched(&portable, context, key, LogicalModifiers::ALT).action,
                Some(expected),
                "portable {context:?}, {key:?}",
            );
        }
        for (key, modifiers) in [
            (LogicalKey::Character('n'), LogicalModifiers::CONTROL),
            (LogicalKey::Character('n'), CONTROL_SHIFT),
            (LogicalKey::Character('N'), LogicalModifiers::CONTROL),
        ] {
            assert_eq!(
                portable
                    .dispatch(
                        &ShortcutContextStack::new([context]),
                        stroke(key, modifiers),
                    )
                    .and_then(|resolved| resolved.action),
                None,
                "portable {context:?}, {key:?}, {modifiers:?}",
            );
        }
    }
    assert_eq!(
        portable.compact_help_label(Context::Board, &[Action::InsertAbove]),
        "Alt+↑",
    );
}

#[test]
fn insert_aliases_never_intercept_text_entry_or_unicode_option_output() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for context in [Context::Compose, Context::Edit, Context::Invocation] {
            for (character, modifiers) in [
                ('j', LogicalModifiers::ALT),
                ('k', LogicalModifiers::ALT),
                ('∆', LogicalModifiers::NONE),
                ('˚', LogicalModifiers::NONE),
            ] {
                let resolved = dispatched(
                    &registry,
                    context,
                    LogicalKey::Character(character),
                    modifiers,
                );
                assert_eq!(resolved.action, None, "{platform:?}, {context:?}");
                assert_eq!(resolved.intention, UiKey::Character(character));
            }
        }
    }

    let macos = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid macOS registry");
    for context in [Context::Compose, Context::Edit, Context::Invocation] {
        for (key, modifiers) in [
            (LogicalKey::Character('n'), LogicalModifiers::CONTROL),
            (LogicalKey::Character('n'), CONTROL_SHIFT),
            (LogicalKey::Character('N'), LogicalModifiers::CONTROL),
        ] {
            assert_eq!(
                macos
                    .dispatch(
                        &ShortcutContextStack::new([context]),
                        stroke(key, modifiers),
                    )
                    .and_then(|resolved| resolved.action),
                None,
                "macOS {context:?}, {key:?}, {modifiers:?}",
            );
        }
    }
}
