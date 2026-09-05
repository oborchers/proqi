//! Compatibility edges shared across contexts and modifier families.

use crate::ui::{
    KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack,
};

use super::super::{ShortcutPlatform, ShortcutRegistry};
use super::{dispatch::dispatched, stroke};

#[test]
fn modified_escape_preserves_the_established_close_route() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for context in super::super::inventory::ESCAPE_CONTEXTS {
        for modifiers in [
            LogicalModifiers::SHIFT,
            LogicalModifiers::ALT,
            LogicalModifiers::CONTROL,
            LogicalModifiers::SUPER.union(LogicalModifiers::META),
            LogicalModifiers::HYPER,
        ] {
            assert_eq!(
                dispatched(&registry, *context, LogicalKey::Escape, modifiers).action,
                Some(Action::Close),
                "context {context:?}, modifiers {modifiers:?}",
            );
        }
    }
}

#[test]
fn page_keys_keep_their_global_fast_navigation_identity() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for context in super::super::inventory::bindings::vocabulary::KEYBOARD_CONTEXTS {
        for (key, expected) in [
            (LogicalKey::PageUp, Action::FastPrevious),
            (LogicalKey::PageDown, Action::FastNext),
        ] {
            assert_eq!(
                dispatched(&registry, *context, key, LogicalModifiers::NONE).action,
                Some(expected),
                "context {context:?}, key {key:?}",
            );
        }
    }
}

#[test]
fn hyper_preserves_named_key_behavior_without_becoming_primary() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for (context, key, expected) in [
        (Context::Edit, LogicalKey::Backspace, Action::Backspace),
        (Context::Edit, LogicalKey::Delete, Action::DeleteForward),
        (Context::Board, LogicalKey::Up, Action::FocusPrevious),
        (Context::Board, LogicalKey::PageUp, Action::FastPrevious),
    ] {
        assert_eq!(
            dispatched(&registry, context, key, LogicalModifiers::HYPER).action,
            Some(expected),
            "context {context:?}, key {key:?}",
        );
    }
    assert_eq!(
        registry.dispatch(
            &ShortcutContextStack::new([Context::Edit]),
            stroke(LogicalKey::Character('v'), LogicalModifiers::HYPER),
        ),
        None,
    );
}
