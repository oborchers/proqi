//! Whole-thought reflow bindings, configurable ownership, and presentation.

use super::super::{ShortcutPlatform, ShortcutRegistry};
use crate::ui::{
    KeyBindings, KeyStroke, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack, UiKey,
};

fn resolve(
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
fn cleanup_defaults_are_board_f_and_portable_control_shift_f() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("registry");
        assert_eq!(
            resolve(
                &registry,
                Context::Board,
                LogicalKey::Character('f'),
                LogicalModifiers::NONE
            ),
            Some(Action::ReflowThought)
        );
        let shifted_control = LogicalModifiers::CONTROL.union(LogicalModifiers::SHIFT);
        for (key, modifiers, expected) in [
            ('f', shifted_control, true),
            ('F', LogicalModifiers::CONTROL, true),
            ('F', shifted_control, true),
            ('f', LogicalModifiers::CONTROL, false),
            ('f', LogicalModifiers::SUPER, false),
            ('f', LogicalModifiers::META, false),
            ('f', LogicalModifiers::ALT, false),
            ('f', LogicalModifiers::SHIFT, false),
            ('f', LogicalModifiers::NONE, false),
        ] {
            assert_eq!(
                resolve(
                    &registry,
                    Context::Edit,
                    LogicalKey::Character(key),
                    modifiers
                ),
                expected.then_some(Action::ReflowThought)
            );
        }
        for modifiers in [
            LogicalModifiers::SUPER,
            LogicalModifiers::META,
            LogicalModifiers::ALT,
            LogicalModifiers::SHIFT,
            LogicalModifiers::NONE,
        ] {
            assert_eq!(
                resolve(
                    &registry,
                    Context::Edit,
                    LogicalKey::Character('F'),
                    modifiers
                ),
                None
            );
        }
        let literal = registry
            .dispatch(
                &ShortcutContextStack::new([Context::Edit]),
                KeyStroke::press(LogicalKey::Character('f')),
            )
            .expect("literal");
        assert_eq!(literal.intention, UiKey::Character('f'));
        assert_eq!(
            registry.labels(Context::Board, Action::ReflowThought),
            ["f"]
        );
        assert_eq!(
            registry.labels(Context::Edit, Action::ReflowThought),
            ["Ctrl+Shift+F"]
        );
        assert_eq!(Action::ReflowThought.diagnostics_id(), "thought.reflow");
        let descriptor = registry
            .descriptor(Action::ReflowThought)
            .expect("descriptor");
        assert!(descriptor.commands.is_some());
        assert!(descriptor.footer.is_none());
        assert_eq!(descriptor.help.len(), 2);
    }
}

#[test]
fn aliases_replace_defaults_and_can_be_disabled_without_removing_commands() {
    let registry = ShortcutRegistry::from_toml(
        r#"schema_version=1
[bindings.board]
"thought.reflow"=[{key="F5"},{key="F6"}]
[bindings.edit]
"thought.reflow"=[]
"#,
    )
    .expect("replacement");
    for key in [LogicalKey::Function(5), LogicalKey::Function(6)] {
        assert_eq!(
            resolve(&registry, Context::Board, key, LogicalModifiers::NONE),
            Some(Action::ReflowThought)
        );
    }
    assert_eq!(
        resolve(
            &registry,
            Context::Board,
            LogicalKey::Character('f'),
            LogicalModifiers::NONE
        ),
        None
    );
    assert!(
        registry
            .labels(Context::Edit, Action::ReflowThought)
            .is_empty()
    );
    assert!(
        registry
            .descriptor(Action::ReflowThought)
            .expect("descriptor")
            .commands
            .is_some()
    );
    for invalid in [
        "schema_version=1\n[bindings.board]\n\"thought.reflow\"=[{key='n'}]",
        "schema_version=1\n[bindings.edit]\n\"thought.reflow\"=[{key='f'}]",
    ] {
        assert!(ShortcutRegistry::from_toml(invalid).is_err());
    }
}

#[test]
fn legacy_board_remapping_keeps_precedence_over_new_fallback() {
    let keys = KeyBindings {
        new: 'f',
        ..KeyBindings::default()
    };
    let registry = ShortcutRegistry::resolve(&keys, ShortcutPlatform::MacOs).expect("legacy map");
    assert_eq!(
        resolve(
            &registry,
            Context::Board,
            LogicalKey::Character('f'),
            LogicalModifiers::NONE
        ),
        Some(Action::New)
    );
}

#[test]
fn existing_legacy_editor_f_bindings_keep_their_exact_precedence() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let keys = KeyBindings {
            transform: 'f',
            delete_sentence: 'F',
            ..KeyBindings::default()
        };
        let registry = ShortcutRegistry::resolve(&keys, platform).expect("legacy editor map");
        let primary = if platform == ShortcutPlatform::MacOs {
            LogicalModifiers::SUPER
        } else {
            LogicalModifiers::CONTROL
        };
        for (key, action) in [
            ('f', Action::ContextualTransform),
            ('F', Action::DeleteSentence),
        ] {
            assert_eq!(
                resolve(
                    &registry,
                    Context::Edit,
                    LogicalKey::Character(key),
                    primary
                ),
                Some(action)
            );
        }
    }
}
