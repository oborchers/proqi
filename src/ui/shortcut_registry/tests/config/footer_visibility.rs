//! Footer-toggle defaults, compatibility, and contextual configuration.

use super::*;

#[test]
fn footer_toggle_has_a_remappable_board_only_default_without_modal_claims() {
    let registry = parse("schema_version=1\n[bindings.board]\n\"footer.toggle\"=[{key='F35'}]")
        .expect("valid Board footer-toggle binding");
    assert_eq!(
        action(
            &registry,
            Context::Board,
            LogicalKey::Function(35),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
    );
    assert_eq!(
        action(
            &registry,
            Context::InsertionBoundary,
            LogicalKey::Function(35),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
    );
    assert_eq!(
        action(
            &registry,
            Context::InsertionBoundary,
            LogicalKey::Character('h'),
            LogicalModifiers::NONE,
        ),
        None,
    );
    let defaults = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid default registry");
    assert_eq!(
        action(
            &defaults,
            Context::Board,
            LogicalKey::Character('h'),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
    );
    assert_eq!(
        action(
            &defaults,
            Context::InsertionBoundary,
            LogicalKey::Character('h'),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
    );
    for context in [Context::Compose, Context::Edit, Context::Recovery] {
        assert_eq!(
            action(
                &defaults,
                context,
                LogicalKey::Character('h'),
                LogicalModifiers::NONE,
            ),
            None,
            "footer toggle must remain Board-owned in {context:?}",
        );
    }
    for context in ["compose", "edit"] {
        assert!(matches!(
            parse(&format!(
                "schema_version=1\n[bindings.{context}]\n\"footer.toggle\"=[{{key='F35'}}]"
            )),
            Err(Error::UnsupportedContext {
                action: Action::ToggleFooter,
                ..
            })
        ));
    }
}

#[test]
fn legacy_board_h_binding_keeps_precedence_over_the_new_factory_footer_toggle() {
    let registry = ShortcutRegistry::resolve(
        &KeyBindings {
            collapse: 'h',
            ..KeyBindings::default()
        },
        ShortcutPlatform::Portable,
    )
    .expect("legacy Board h remains a valid configuration");
    for context in [Context::Board, Context::InsertionBoundary] {
        assert_eq!(
            action(
                &registry,
                context,
                LogicalKey::Character('h'),
                LogicalModifiers::NONE,
            ),
            Some(Action::Collapse),
        );
    }
}

#[test]
fn versioned_board_h_binding_displaces_the_footer_default_in_its_own_context() {
    let registry = parse("schema_version=1\n[bindings.board]\n\"thought.collapse\"=[{key='h'}]")
        .expect("an existing Board h keymap remains valid");
    assert_eq!(
        action(
            &registry,
            Context::Board,
            LogicalKey::Character('h'),
            LogicalModifiers::NONE,
        ),
        Some(Action::Collapse),
    );
    assert_eq!(
        action(
            &registry,
            Context::InsertionBoundary,
            LogicalKey::Character('h'),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
        "a Board-only override leaves the separately configurable insertion boundary unchanged",
    );
}

#[test]
fn explicit_insertion_footer_binding_overrides_the_projected_board_binding() {
    let registry = parse(
        "schema_version=1\n[bindings.board]\n\"footer.toggle\"=[{key='F35'}]\n[bindings.insertion_boundary]\n\"footer.toggle\"=[{key='F34'}]",
    )
    .expect("independent footer bindings remain valid");
    assert_eq!(
        action(
            &registry,
            Context::Board,
            LogicalKey::Function(35),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
    );
    assert_eq!(
        action(
            &registry,
            Context::InsertionBoundary,
            LogicalKey::Function(34),
            LogicalModifiers::NONE,
        ),
        Some(Action::ToggleFooter),
    );
    assert_eq!(
        action(
            &registry,
            Context::InsertionBoundary,
            LogicalKey::Function(35),
            LogicalModifiers::NONE,
        ),
        None,
    );
}
