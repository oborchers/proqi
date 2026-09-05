use crate::ui::{KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action};

use super::super::{ShortcutContext, ShortcutContextStack, ShortcutPlatform, ShortcutRegistry};
use super::stroke;

fn action(
    platform: ShortcutPlatform,
    modifiers: LogicalModifiers,
    key: LogicalKey,
) -> Option<Action> {
    ShortcutRegistry::resolve(&KeyBindings::default(), platform)
        .expect("valid registry")
        .dispatch(
            &ShortcutContextStack::new([ShortcutContext::Edit]),
            stroke(key, modifiers),
        )
        .and_then(|resolved| resolved.action)
}

fn configured_action(
    keys: &KeyBindings,
    platform: ShortcutPlatform,
    modifiers: LogicalModifiers,
    key: LogicalKey,
) -> Option<Action> {
    ShortcutRegistry::resolve(keys, platform)
        .expect("valid registry")
        .dispatch(
            &ShortcutContextStack::new([ShortcutContext::Edit]),
            stroke(key, modifiers),
        )
        .and_then(|resolved| resolved.action)
}

#[test]
fn primary_expands_to_super_or_meta_on_macos_but_never_raw_control() {
    for modifier in [LogicalModifiers::SUPER, LogicalModifiers::META] {
        assert_eq!(
            action(
                ShortcutPlatform::MacOs,
                modifier,
                LogicalKey::Character('a')
            ),
            Some(Action::SelectAll)
        );
    }
    assert_eq!(
        action(
            ShortcutPlatform::MacOs,
            LogicalModifiers::CONTROL,
            LogicalKey::Character('a')
        ),
        None
    );
}

#[test]
fn primary_expands_only_to_control_on_portable_platforms() {
    assert_eq!(
        action(
            ShortcutPlatform::Portable,
            LogicalModifiers::CONTROL,
            LogicalKey::Character('a')
        ),
        Some(Action::SelectAll)
    );
    for modifier in [LogicalModifiers::SUPER, LogicalModifiers::META] {
        assert_eq!(
            action(
                ShortcutPlatform::Portable,
                modifier,
                LogicalKey::Character('a')
            ),
            None
        );
    }
}

#[test]
fn raw_modifiers_and_mixed_chords_remain_distinct() {
    for (platform, modifiers) in [
        (
            ShortcutPlatform::MacOs,
            LogicalModifiers::SUPER.union(LogicalModifiers::CONTROL),
        ),
        (
            ShortcutPlatform::MacOs,
            LogicalModifiers::SUPER.union(LogicalModifiers::META),
        ),
        (
            ShortcutPlatform::Portable,
            LogicalModifiers::CONTROL.union(LogicalModifiers::ALT),
        ),
        (
            ShortcutPlatform::Portable,
            LogicalModifiers::CONTROL.union(LogicalModifiers::SUPER),
        ),
    ] {
        assert_eq!(
            action(platform, modifiers, LogicalKey::Character('v')),
            None
        );
        assert_eq!(action(platform, modifiers, LogicalKey::Enter), None);
    }
}

#[test]
fn uppercase_without_distinct_shift_keeps_existing_compatibility() {
    for (platform, primary) in [
        (ShortcutPlatform::MacOs, LogicalModifiers::SUPER),
        (ShortcutPlatform::MacOs, LogicalModifiers::META),
        (ShortcutPlatform::Portable, LogicalModifiers::CONTROL),
    ] {
        assert_eq!(
            action(platform, primary, LogicalKey::Character('A')),
            Some(Action::SelectAll)
        );
        assert_eq!(
            action(
                platform,
                primary.union(LogicalModifiers::SHIFT),
                LogicalKey::Character('A')
            ),
            None
        );
    }
}

#[test]
fn configured_shifted_actions_accept_uppercase_reports_without_a_shift_bit() {
    let keys = KeyBindings {
        delete_sentence: 'T',
        ..KeyBindings::default()
    };
    for (platform, primary) in [
        (ShortcutPlatform::MacOs, LogicalModifiers::SUPER),
        (ShortcutPlatform::MacOs, LogicalModifiers::META),
        (ShortcutPlatform::Portable, LogicalModifiers::CONTROL),
    ] {
        for (character, expected) in [
            ('T', Action::DeleteSentence),
            ('H', Action::ExtendVisualRowStart),
            ('L', Action::ExtendVisualRowEnd),
        ] {
            assert_eq!(
                configured_action(&keys, platform, primary, LogicalKey::Character(character),),
                Some(expected),
            );
        }
    }
}
