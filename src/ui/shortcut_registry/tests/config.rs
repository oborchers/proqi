//! Versioned keymap aliases, platform overrides, collisions, and safety.

use super::*;
use crate::ui::{KeyStroke, LogicalKey, ShortcutContextStack};
use std::fmt::Write as _;

fn parse(content: &str) -> Result<ShortcutRegistry, Error> {
    toml::from_str::<KeymapDocument>(content)
        .expect("valid TOML")
        .resolve(ShortcutPlatform::MacOs)
}

fn action(
    registry: &ShortcutRegistry,
    context: Context,
    key: LogicalKey,
    modifiers: LogicalModifiers,
) -> Option<Action> {
    registry
        .dispatch(
            &ShortcutContextStack::new([context]),
            KeyStroke::press(key).with_modifiers(modifiers),
        )
        .and_then(|resolved| resolved.action)
}

#[test]
fn multiple_aliases_replace_primary_and_board_fallback_together() {
    let registry = parse(
        r#"
schema_version = 1
[bindings.board]
"submission.submit_remove" = [{key="F5"}, {key="Enter", modifiers=["Control", "Alt"]}]
"submission.submit_keep" = []
"thought.delete" = [{key="F6"}]
"context.close" = [{key="Escape"}, {key="F7"}]
"#,
    )
    .expect("valid replacements");
    for (key, modifiers, expected) in [
        (
            LogicalKey::Function(5),
            LogicalModifiers::NONE,
            Some(Action::SubmitRemove),
        ),
        (
            LogicalKey::Enter,
            LogicalModifiers::CONTROL.union(LogicalModifiers::ALT),
            Some(Action::SubmitRemove),
        ),
        (LogicalKey::Enter, LogicalModifiers::SUPER, None),
        (LogicalKey::Character('s'), LogicalModifiers::NONE, None),
        (LogicalKey::Character('S'), LogicalModifiers::NONE, None),
        (LogicalKey::Delete, LogicalModifiers::NONE, None),
        (
            LogicalKey::Function(6),
            LogicalModifiers::NONE,
            Some(Action::Delete),
        ),
    ] {
        assert_eq!(action(&registry, Context::Board, key, modifiers), expected);
    }
    assert_eq!(
        action(
            &registry,
            Context::Edit,
            LogicalKey::Enter,
            LogicalModifiers::SUPER
        ),
        Some(Action::SubmitRemove)
    );
}

#[test]
fn platform_override_replaces_common_alias_list_and_primary_expands_exactly() {
    let document: KeymapDocument = toml::from_str(
        r#"
schema_version = 1
[bindings.edit]
"submission.submit_remove" = [{key="F5"}]
[macos.edit]
"submission.submit_remove" = [{key="p", modifiers=["Primary", "Shift"]}]
[portable.edit]
"submission.submit_remove" = [{key="g", modifiers=["Primary"]}]
"#,
    )
    .expect("document");
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry = document.resolve(platform).expect("platform keymap");
        for modifier in [
            LogicalModifiers::SUPER,
            LogicalModifiers::META,
            LogicalModifiers::CONTROL,
        ] {
            let (key, modifiers, expected) = if platform == ShortcutPlatform::MacOs {
                (
                    'p',
                    modifier.union(LogicalModifiers::SHIFT),
                    modifier != LogicalModifiers::CONTROL,
                )
            } else {
                ('g', modifier, modifier == LogicalModifiers::CONTROL)
            };
            assert_eq!(
                action(
                    &registry,
                    Context::Edit,
                    LogicalKey::Character(key),
                    modifiers
                ),
                expected.then_some(Action::SubmitRemove)
            );
        }
        assert_eq!(
            action(
                &registry,
                Context::Edit,
                LogicalKey::Function(5),
                LogicalModifiers::NONE
            ),
            None
        );
    }
}

#[test]
fn invalid_version_and_unknown_identities_fail_with_typed_redacted_errors() {
    assert_eq!(parse("schema_version=2"), Err(Error::UnsupportedVersion(2)));
    assert!(matches!(
        parse("schema_version=1\n[bindings.private_path]\n"),
        Err(Error::UnknownContext(0))
    ));
    assert!(matches!(
        parse("schema_version=1\n[bindings.board]\nprivate_secret=[]"),
        Err(Error::UnknownAction { .. })
    ));
    for alias in [
        "{key='secret-long-key'}",
        "{key='F0'}",
        "{key='f',modifiers=['Command']}",
        "{key='f',modifiers=['Alt','Alt']}",
        "{key='f',modifiers=['Primary','Super']}",
    ] {
        let error = parse(&format!(
            "schema_version=1\n[bindings.board]\n\"thought.new\"=[{alias}]"
        ))
        .expect_err("malformed alias");
        assert!(matches!(error, Error::MalformedAlias { .. }));
        assert!(!error.to_string().contains("secret"));
    }
}

#[test]
fn collisions_text_theft_escape_loss_and_recovery_loss_are_rejected() {
    for (context, name, aliases) in [
        ("board", "thought.new", "[{key='e'}]"),
        ("edit", "submission.submit_remove", "[{key='a'}]"),
        (
            "edit",
            "submission.submit_remove",
            "[{key='A',modifiers=['Shift']}]",
        ),
        ("help", "context.close", "[]"),
        ("recovery", "recovery.export", "[]"),
    ] {
        assert!(
            parse(&format!(
                "schema_version=1\n[bindings.{context}]\n\"{name}\"={aliases}"
            ))
            .is_err()
        );
    }
}

#[test]
fn same_chord_coexists_in_distinct_owners() {
    parse(
        r#"schema_version=1
[bindings.board]
"thought.new"=[{key="F5"}]
[bindings.edit]
"submission.submit_remove"=[{key="F5"}]
"#,
    )
    .expect("context separation");
}

#[test]
fn every_eligible_action_context_pair_is_configurable_and_resolves_its_identity() {
    let descriptors = inventory::descriptors(&KeyBindings::default());
    for descriptor in descriptors {
        let aliases = if descriptor.action == Action::Close {
            "[{key='Escape'},{key='F35'}]"
        } else {
            "[{key='F35'}]"
        };
        let mut document = String::from("schema_version=1\n");
        for context in descriptor.contexts {
            write!(
                document,
                "[bindings.{}]\n\"{}\"={aliases}\n",
                context.configuration_id(),
                descriptor.diagnostics
            )
            .unwrap();
        }
        let registry =
            parse(&document).unwrap_or_else(|error| panic!("{}: {error}", descriptor.diagnostics));
        for context in registry
            .descriptor(descriptor.action)
            .unwrap()
            .contexts
            .iter()
            .copied()
        {
            assert_eq!(
                action(
                    &registry,
                    context,
                    LogicalKey::Function(35),
                    LogicalModifiers::NONE
                ),
                Some(descriptor.action),
                "{} in {context:?}",
                descriptor.diagnostics
            );
        }
    }
}

#[test]
fn altgr_option_and_shifted_text_cannot_be_stolen_from_any_text_owner() {
    for context in super::super::inventory::bindings::vocabulary::KEYBOARD_CONTEXTS
        .iter()
        .copied()
        .filter(|context| super::super::inventory::bindings::vocabulary::is_text_context(*context))
    {
        for modifiers in [
            "",
            "'Shift'",
            "'Alt'",
            "'Control','Alt'",
            "'Control','Alt','Shift'",
        ] {
            let document = format!(
                "schema_version=1\n[bindings.{}]\n\"context.close\"=[{{key='Escape'}},{{key='ä',modifiers=[{modifiers}]}}]",
                context.configuration_id()
            );
            assert!(
                matches!(parse(&document), Err(Error::TextInputTheft { .. })),
                "{context:?} {modifiers}"
            );
        }
    }
}

#[test]
fn explicit_control_alt_shift_super_meta_and_hyper_are_independent() {
    let registry = parse("schema_version=1\n[bindings.board]\n\"thought.new\"=[{key='F35',modifiers=['Control','Alt','Shift','Super','Meta','Hyper']}]").unwrap();
    let all = LogicalModifiers::CONTROL
        .union(LogicalModifiers::ALT)
        .union(LogicalModifiers::SHIFT)
        .union(LogicalModifiers::SUPER)
        .union(LogicalModifiers::META)
        .union(LogicalModifiers::HYPER);
    assert_eq!(
        action(&registry, Context::Board, LogicalKey::Function(35), all),
        Some(Action::New)
    );
    for modifier in [
        LogicalModifiers::CONTROL,
        LogicalModifiers::ALT,
        LogicalModifiers::SHIFT,
        LogicalModifiers::SUPER,
        LogicalModifiers::META,
        LogicalModifiers::HYPER,
    ] {
        assert_eq!(
            action(
                &registry,
                Context::Board,
                LogicalKey::Function(35),
                all.difference(modifier)
            ),
            None
        );
    }
}

#[test]
fn duplicate_aliases_and_disabling_required_exit_discovery_are_rejected() {
    assert!(matches!(
        parse("schema_version=1\n[bindings.board]\n\"thought.new\"=[{key='F5'},{key='F5'}]"),
        Err(Error::DuplicateAlias { .. })
    ));
    for (context, name) in [
        ("board", "commands.open"),
        ("board", "application.quit"),
        ("insertion_boundary", "commands.open"),
        ("insertion_boundary", "application.quit"),
    ] {
        assert!(matches!(
            parse(&format!(
                "schema_version=1\n[bindings.{context}]\n\"{name}\"=[]"
            )),
            Err(Error::UnreachableAction { .. })
        ));
    }
    assert!(
        ShortcutRegistry::from_toml(
            "schema_version=1\n[bindings.board]\nsecret=[{key='private',extra='secret'}]"
        )
        .unwrap_err()
        .to_string()
        .find("secret")
        .is_none()
    );
}

#[test]
fn exact_uppercase_aliases_have_distinct_help_and_footer_labels() {
    let registry = parse("schema_version=1\n[bindings.board]\n\"board.duplicate\"=[]\n\"thought.new\"=[{key='d',modifiers=['Control']}]\n\"thought.delete\"=[{key='D',modifiers=['Control']}]").unwrap();
    assert_eq!(
        registry.action_label(Context::Board, Action::New, false),
        "Ctrl+D"
    );
    assert_eq!(
        registry.action_label(Context::Board, Action::Delete, false),
        "Ctrl+U+0044"
    );
    assert_eq!(
        registry.help_label(Context::Board, &[Action::Delete]),
        "Ctrl+U+0044"
    );
    let same = parse("schema_version=1\n[bindings.board]\n\"board.duplicate\"=[]\n\"thought.new\"=[{key='d',modifiers=['Control']},{key='D',modifiers=['Control']}]").unwrap();
    assert_eq!(
        same.action_label(Context::Board, Action::New, false),
        "Ctrl+D"
    );
}
