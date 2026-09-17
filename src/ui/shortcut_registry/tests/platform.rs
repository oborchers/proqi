use crate::ui::{KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action};

use super::super::{ShortcutContext, ShortcutContextStack, ShortcutPlatform, ShortcutRegistry};
use super::stroke;

fn action(
    platform: ShortcutPlatform,
    modifiers: LogicalModifiers,
    key: LogicalKey,
) -> Option<Action> {
    context_action(platform, ShortcutContext::Edit, modifiers, key)
}

fn context_action(
    platform: ShortcutPlatform,
    context: ShortcutContext,
    modifiers: LogicalModifiers,
    key: LogicalKey,
) -> Option<Action> {
    ShortcutRegistry::resolve(&KeyBindings::default(), platform)
        .expect("valid registry")
        .dispatch(
            &ShortcutContextStack::new([context]),
            stroke(key, modifiers),
        )
        .and_then(|resolved| resolved.action)
}

#[test]
fn macos_option_shift_is_an_exact_board_reorder_alias_only() {
    let option_shift = LogicalModifiers::ALT.union(LogicalModifiers::SHIFT);
    for context in [ShortcutContext::Board, ShortcutContext::InsertionBoundary] {
        for (key, expected) in [
            (LogicalKey::Up, Action::MoveUp),
            (LogicalKey::Character('k'), Action::MoveUp),
            (LogicalKey::Character('K'), Action::MoveUp),
            (LogicalKey::Down, Action::MoveDown),
            (LogicalKey::Character('j'), Action::MoveDown),
            (LogicalKey::Character('J'), Action::MoveDown),
        ] {
            assert_eq!(
                context_action(ShortcutPlatform::MacOs, context, option_shift, key),
                Some(expected),
                "{context:?}, {key:?}",
            );
        }
    }

    for context in [ShortcutContext::Compose, ShortcutContext::Edit] {
        assert_eq!(
            context_action(
                ShortcutPlatform::MacOs,
                context,
                option_shift,
                LogicalKey::Up,
            ),
            Some(Action::FastExtendPrevious),
        );
    }
    assert_eq!(
        context_action(
            ShortcutPlatform::Portable,
            ShortcutContext::Board,
            option_shift,
            LogicalKey::Up,
        ),
        Some(Action::ExtendPrevious),
    );
}

#[test]
fn macos_option_shift_reorders_through_remapped_vertical_board_keys() {
    let keys = KeyBindings {
        focus_up: 'b',
        focus_down: 'g',
        range_up: 'B',
        range_down: 'G',
        ..KeyBindings::default()
    };
    let registry =
        ShortcutRegistry::resolve(&keys, ShortcutPlatform::MacOs).expect("valid remapped registry");
    let contexts = [ShortcutContext::Board, ShortcutContext::InsertionBoundary];
    let option_shift = LogicalModifiers::ALT.union(LogicalModifiers::SHIFT);
    for context in contexts {
        for (key, expected) in [
            (LogicalKey::Character('b'), Action::MoveUp),
            (LogicalKey::Character('B'), Action::MoveUp),
            (LogicalKey::Character('g'), Action::MoveDown),
            (LogicalKey::Character('G'), Action::MoveDown),
        ] {
            assert_eq!(
                registry
                    .dispatch(
                        &ShortcutContextStack::new([context]),
                        stroke(key, option_shift)
                    )
                    .and_then(|resolved| resolved.action),
                Some(expected),
                "{context:?}, {key:?}",
            );
        }
    }
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
fn physical_control_r_is_the_exact_name_editor_shortcut_on_both_platforms() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        for context in [ShortcutContext::Board, ShortcutContext::Edit] {
            assert_eq!(
                context_action(
                    platform,
                    context,
                    LogicalModifiers::CONTROL,
                    LogicalKey::Character('r'),
                ),
                Some(Action::RenameThought)
            );
        }
        assert_eq!(
            context_action(
                platform,
                ShortcutContext::Compose,
                LogicalModifiers::CONTROL,
                LogicalKey::Character('r'),
            ),
            None
        );
    }
}

#[test]
fn horizontal_primary_arrows_keep_platform_specific_movement_contracts() {
    for modifier in [LogicalModifiers::SUPER, LogicalModifiers::META] {
        assert_eq!(
            action(ShortcutPlatform::MacOs, modifier, LogicalKey::Left),
            Some(Action::MoveVisualRowStart)
        );
        assert_eq!(
            action(ShortcutPlatform::MacOs, modifier, LogicalKey::Right),
            Some(Action::MoveVisualRowEnd)
        );
    }
    assert_eq!(
        action(
            ShortcutPlatform::Portable,
            LogicalModifiers::CONTROL,
            LogicalKey::Left
        ),
        Some(Action::MoveWordBack)
    );
    assert_eq!(
        action(
            ShortcutPlatform::Portable,
            LogicalModifiers::CONTROL,
            LogicalKey::Right
        ),
        Some(Action::MoveWordForward)
    );
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
