//! Terminal-safe thought, line, and Board boundary shortcut contracts.

use crate::ui::{
    KeyBindings, KeyPhase, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack, UiKey,
};

use super::super::{HelpSurface, ShortcutPlatform, ShortcutRegistry};
use super::{dispatch::dispatched, stroke};

const CONTROL_SHIFT: LogicalModifiers = LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT);
const ALT_SHIFT: LogicalModifiers = LogicalModifiers::ALT.union(LogicalModifiers::SHIFT);

#[test]
fn logical_control_owns_complete_thought_navigation_on_every_platform() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for context in [Context::Compose, Context::Edit] {
            for (key, modifiers, expected) in [
                (
                    LogicalKey::Up,
                    LogicalModifiers::CONTROL,
                    Action::MoveDocumentStart,
                ),
                (
                    LogicalKey::Down,
                    LogicalModifiers::CONTROL,
                    Action::MoveDocumentEnd,
                ),
                (LogicalKey::Up, CONTROL_SHIFT, Action::ExtendDocumentStart),
                (LogicalKey::Down, CONTROL_SHIFT, Action::ExtendDocumentEnd),
            ] {
                assert_eq!(
                    dispatched(&registry, context, key, modifiers).action,
                    Some(expected),
                    "{platform:?}, {context:?}, {key:?}, {modifiers:?}",
                );
            }
        }
    }
}

#[test]
fn invocation_modal_navigation_precedes_control_thought_boundaries() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for (key, expected) in [
            (LogicalKey::Up, Action::FocusPrevious),
            (LogicalKey::Down, Action::FocusNext),
        ] {
            for modifiers in [LogicalModifiers::CONTROL, CONTROL_SHIFT] {
                assert_eq!(
                    dispatched(&registry, Context::Invocation, key, modifiers).action,
                    Some(expected),
                );
            }
        }
    }
}

#[test]
fn macos_super_and_meta_no_longer_claim_complete_thought_navigation() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for modifiers in [LogicalModifiers::SUPER, LogicalModifiers::META] {
        for (key, expected) in [
            (LogicalKey::Up, Action::MoveVisualUp),
            (LogicalKey::Down, Action::MoveVisualDown),
        ] {
            assert_eq!(
                dispatched(&registry, Context::Edit, key, modifiers).action,
                Some(expected),
            );
        }
    }
}

#[test]
fn directional_line_defaults_are_platform_safe_and_home_end_remain_aliases() {
    for (platform, modifier) in [
        (ShortcutPlatform::MacOs, LogicalModifiers::CONTROL),
        (ShortcutPlatform::Portable, LogicalModifiers::ALT),
    ] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for (key, shifted, expected) in [
            (LogicalKey::Left, false, Action::MoveLineStart),
            (LogicalKey::Right, false, Action::MoveLineEnd),
            (LogicalKey::Left, true, Action::ExtendLineStart),
            (LogicalKey::Right, true, Action::ExtendLineEnd),
        ] {
            let modifiers = if shifted {
                modifier.union(LogicalModifiers::SHIFT)
            } else {
                modifier
            };
            assert_eq!(
                dispatched(&registry, Context::Edit, key, modifiers).action,
                Some(expected),
                "{platform:?}, {key:?}, {modifiers:?}",
            );
        }
        for (key, modifiers, expected) in [
            (
                LogicalKey::Home,
                LogicalModifiers::NONE,
                Action::MoveLineStart,
            ),
            (LogicalKey::End, LogicalModifiers::NONE, Action::MoveLineEnd),
            (
                LogicalKey::Home,
                LogicalModifiers::SHIFT,
                Action::ExtendLineStart,
            ),
            (
                LogicalKey::End,
                LogicalModifiers::SHIFT,
                Action::ExtendLineEnd,
            ),
        ] {
            assert_eq!(
                dispatched(&registry, Context::Edit, key, modifiers).action,
                Some(expected),
            );
        }
        assert_eq!(
            registry.compact_help_label(Context::Edit, &[Action::MoveLineStart]),
            if platform == ShortcutPlatform::MacOs {
                "Ctrl+←"
            } else {
                "Alt+←"
            }
        );
    }
}

#[test]
fn board_boundaries_insertions_and_configured_vertical_aliases_are_typed() {
    let remapped = KeyBindings {
        focus_up: 'b',
        focus_down: 'g',
        range_up: 'B',
        range_down: 'G',
        ..KeyBindings::default()
    };
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry = ShortcutRegistry::resolve(&remapped, platform).expect("valid remap");
        for context in [Context::Board, Context::InsertionBoundary] {
            for (key, modifiers, expected) in [
                (
                    LogicalKey::Up,
                    LogicalModifiers::CONTROL,
                    Action::FocusFirst,
                ),
                (
                    LogicalKey::Character('b'),
                    LogicalModifiers::CONTROL,
                    Action::FocusFirst,
                ),
                (
                    LogicalKey::Down,
                    LogicalModifiers::CONTROL,
                    Action::FocusLast,
                ),
                (
                    LogicalKey::Character('g'),
                    LogicalModifiers::CONTROL,
                    Action::FocusLast,
                ),
                (LogicalKey::Up, LogicalModifiers::ALT, Action::InsertAbove),
                (
                    LogicalKey::Character('b'),
                    LogicalModifiers::ALT,
                    Action::InsertAbove,
                ),
                (LogicalKey::Down, LogicalModifiers::ALT, Action::InsertBelow),
                (
                    LogicalKey::Character('g'),
                    LogicalModifiers::ALT,
                    Action::InsertBelow,
                ),
            ] {
                assert_eq!(
                    dispatched(&registry, context, key, modifiers).action,
                    Some(expected),
                    "{platform:?}, {context:?}, {key:?}",
                );
            }
        }
    }
}

#[test]
fn shifted_board_boundary_policy_preserves_portable_reorder_and_macos_range() {
    for (platform, modifiers, expected_up, expected_down) in [
        (
            ShortcutPlatform::MacOs,
            CONTROL_SHIFT,
            Action::ExtendFirst,
            Action::ExtendLast,
        ),
        (
            ShortcutPlatform::Portable,
            CONTROL_SHIFT,
            Action::MoveUp,
            Action::MoveDown,
        ),
        (
            ShortcutPlatform::MacOs,
            ALT_SHIFT,
            Action::MoveUp,
            Action::MoveDown,
        ),
        (
            ShortcutPlatform::Portable,
            ALT_SHIFT,
            Action::ExtendPrevious,
            Action::ExtendNext,
        ),
    ] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for (key, expected) in [
            (LogicalKey::Up, expected_up),
            (LogicalKey::Down, expected_down),
        ] {
            assert_eq!(
                dispatched(&registry, Context::Board, key, modifiers).action,
                Some(expected),
                "{platform:?}, {key:?}, {modifiers:?}",
            );
        }
    }
}

#[test]
fn macos_uppercase_control_reports_keep_shifted_boundary_compatibility() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for (character, expected) in [('K', Action::ExtendFirst), ('J', Action::ExtendLast)] {
        assert_eq!(
            dispatched(
                &registry,
                Context::Board,
                LogicalKey::Character(character),
                LogicalModifiers::CONTROL,
            )
            .action,
            Some(expected),
        );
    }
}

#[test]
fn shifted_board_boundaries_have_platform_truthful_replaceable_help() {
    let macos = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    let portable = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    for (action, expected) in [
        (Action::ExtendFirst, "Ctrl+Shift+↑"),
        (Action::ExtendLast, "Ctrl+Shift+↓"),
    ] {
        assert_eq!(macos.help_labels(Context::Board, &[action]), [expected]);
        assert!(portable.help_labels(Context::Board, &[action]).is_empty());
    }

    let replaced = ShortcutRegistry::from_toml(
        "schema_version=1\n[bindings.board]\n\"board.range_first_thought\"=[{key='F5'}]\n\"board.range_last_thought\"=[]",
    )
    .expect("safe help replacement");
    assert_eq!(
        replaced.help_labels(Context::Board, &[Action::ExtendFirst]),
        ["F5"]
    );
    assert!(
        replaced
            .help_labels(Context::Board, &[Action::ExtendLast])
            .is_empty()
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
}

fn replaced_and_disabled_registry() -> ShortcutRegistry {
    ShortcutRegistry::from_toml(
        r#"schema_version=1
[bindings.board]
"board.first_thought"=[{key="F5"},{key="F6"}]
"board.last_thought"=[]
"thought.insert_above"=[{key="F7"}]
"thought.insert_below"=[]
"board.range_first_thought"=[]
"board.range_last_thought"=[]
"#,
    )
    .expect("safe replacement")
}

#[test]
fn new_actions_are_replaceable_with_multiple_aliases() {
    let registry = replaced_and_disabled_registry();
    for key in [LogicalKey::Function(5), LogicalKey::Function(6)] {
        assert_eq!(
            dispatched(&registry, Context::Board, key, LogicalModifiers::NONE).action,
            Some(Action::FocusFirst),
        );
    }
    assert_eq!(
        dispatched(
            &registry,
            Context::Board,
            LogicalKey::Function(7),
            LogicalModifiers::NONE,
        )
        .action,
        Some(Action::InsertAbove),
    );
    for (key, modifiers) in [
        (LogicalKey::Up, LogicalModifiers::CONTROL),
        (LogicalKey::Character('k'), LogicalModifiers::CONTROL),
        (LogicalKey::Up, LogicalModifiers::ALT),
        (LogicalKey::Character('k'), LogicalModifiers::ALT),
    ] {
        assert_eq!(
            registry
                .dispatch(
                    &ShortcutContextStack::new([Context::Board]),
                    stroke(key, modifiers),
                )
                .and_then(|resolved| resolved.action),
            None,
            "replacement must remove every previous alias",
        );
    }
}

#[test]
fn new_actions_are_disableable_with_truthful_help() {
    let registry = replaced_and_disabled_registry();
    for action in [
        Action::FocusLast,
        Action::InsertBelow,
        Action::ExtendFirst,
        Action::ExtendLast,
    ] {
        assert!(registry.labels(Context::Board, action).is_empty());
    }
    let visible_help = registry
        .help(HelpSurface::Board)
        .into_iter()
        .filter(|(action, _)| !registry.help_labels(Context::Board, &[*action]).is_empty())
        .map(|(_, metadata)| metadata.label)
        .collect::<Vec<_>>();
    assert!(visible_help.contains(&"Insert above"));
    assert!(visible_help.contains(&"First thought"));
    assert!(!visible_help.contains(&"Insert below"));
    assert!(!visible_help.contains(&"Last thought"));
    for (key, modifiers) in [
        (LogicalKey::Down, LogicalModifiers::CONTROL),
        (LogicalKey::Character('j'), LogicalModifiers::CONTROL),
        (LogicalKey::Down, LogicalModifiers::ALT),
        (LogicalKey::Character('j'), LogicalModifiers::ALT),
    ] {
        assert_eq!(
            registry
                .dispatch(
                    &ShortcutContextStack::new([Context::Board]),
                    stroke(key, modifiers),
                )
                .and_then(|resolved| resolved.action),
            None,
            "disabled defaults and derived aliases must both disappear",
        );
    }
    for (key, portable_action) in [
        (LogicalKey::Up, Action::MoveUp),
        (LogicalKey::Down, Action::MoveDown),
    ] {
        assert_eq!(
            registry
                .dispatch(
                    &ShortcutContextStack::new([Context::Board]),
                    stroke(key, CONTROL_SHIFT),
                )
                .and_then(|resolved| resolved.action),
            (!cfg!(target_os = "macos")).then_some(portable_action),
            "disabling macOS range actions must preserve portable Primary+Shift reorder",
        );
    }
}

#[test]
fn new_actions_reject_invalid_context_and_collision() {
    assert!(
        ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.edit]\n\"thought.insert_above\"=[{key='F5'}]",
        )
        .is_err()
    );

    assert!(
        ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.board]\n\"board.first_thought\"=[{key='F5'}]\n\"thought.insert_above\"=[{key='F5'}]",
        )
        .is_err()
    );
}

#[test]
fn diagnostics_and_repeat_identity_are_stable_for_new_actions() {
    for (action, identity) in [
        (Action::FocusFirst, "board.first_thought"),
        (Action::FocusLast, "board.last_thought"),
        (Action::ExtendFirst, "board.range_first_thought"),
        (Action::ExtendLast, "board.range_last_thought"),
        (Action::InsertAbove, "thought.insert_above"),
        (Action::InsertBelow, "thought.insert_below"),
    ] {
        assert_eq!(action.diagnostics_id(), identity);
    }

    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    let contexts = ShortcutContextStack::new([Context::Board]);
    let mut input = stroke(LogicalKey::Down, LogicalModifiers::ALT);
    input.phase = KeyPhase::Repeat;
    assert_eq!(
        registry
            .dispatch(&contexts, input)
            .and_then(|resolved| resolved.action),
        Some(Action::InsertBelow),
    );
    input.phase = KeyPhase::Release;
    assert_eq!(registry.dispatch(&contexts, input), None);
}
