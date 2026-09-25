//! Plain-text export actions: unbound factory defaults and versioned configuration.

use super::{Action, Context, LogicalKey, LogicalModifiers, action, parse};
use crate::ui::{KeyBindings, shortcut_registry::ShortcutPlatform};

#[test]
fn export_actions_are_unbound_by_default_but_configurable_by_identity() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry = super::ShortcutRegistry::resolve(&KeyBindings::default(), platform)
            .expect("factory registry");
        for export in [
            Action::ExportThoughts,
            Action::ExportRemove,
            Action::ExportReplace,
        ] {
            assert!(
                registry.labels(Context::Board, export).is_empty(),
                "{export:?} has no factory binding on {platform:?}"
            );
        }
    }
    let registry = parse(
        "schema_version=1\n[bindings.board]\n\"thought.export\"=[{key='F5'}]\n\"thought.export_remove\"=[{key='F6'}]\n\"thought.export_replace\"=[{key='F7'}]",
    )
    .expect("valid export bindings");
    for (key, expected) in [
        (5, Action::ExportThoughts),
        (6, Action::ExportRemove),
        (7, Action::ExportReplace),
    ] {
        assert_eq!(
            action(
                &registry,
                Context::Board,
                LogicalKey::Function(key),
                LogicalModifiers::NONE,
            ),
            Some(expected)
        );
    }
}

#[test]
fn the_destination_field_completes_with_tab_and_keeps_letters_as_text() {
    let registry = parse("schema_version=1").expect("factory registry");
    for (key, modifiers, expected) in [
        (LogicalKey::Tab, LogicalModifiers::NONE, Some(Action::Tab)),
        (
            LogicalKey::Tab,
            LogicalModifiers::SHIFT,
            Some(Action::BackTab),
        ),
        (
            LogicalKey::BackTab,
            LogicalModifiers::NONE,
            Some(Action::BackTab),
        ),
        (
            LogicalKey::Enter,
            LogicalModifiers::NONE,
            Some(Action::Confirm),
        ),
        (
            LogicalKey::Escape,
            LogicalModifiers::NONE,
            Some(Action::Close),
        ),
        (LogicalKey::Character('j'), LogicalModifiers::NONE, None),
        (LogicalKey::Character('n'), LogicalModifiers::NONE, None),
    ] {
        assert_eq!(
            action(&registry, Context::ExportPath, key, modifiers),
            expected,
            "{key:?} {modifiers:?}"
        );
    }
    for (key, expected) in [
        (LogicalKey::Character('j'), Action::FocusNext),
        (LogicalKey::Down, Action::FocusNext),
        (LogicalKey::Character('k'), Action::FocusPrevious),
        (LogicalKey::Enter, Action::Confirm),
        (LogicalKey::Escape, Action::Close),
    ] {
        assert_eq!(
            action(
                &registry,
                Context::ExportReplace,
                key,
                LogicalModifiers::NONE
            ),
            Some(expected),
            "{key:?}"
        );
    }
}
