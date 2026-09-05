use crate::ui::{
    KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action, ShortcutBinding,
    ShortcutBindingClaim, ShortcutContext as Context, ShortcutModifiers,
};

use super::super::{
    ShortcutPlatform, ShortcutRegistry,
    validation::{ShortcutRegistryError as Error, validate_descriptors},
};

fn descriptors() -> Vec<crate::ui::ShortcutDescriptor> {
    ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry")
        .descriptors()
        .to_vec()
}

fn plain(character: char) -> ShortcutBinding {
    ShortcutBinding {
        key: LogicalKey::Character(character),
        modifiers: ShortcutModifiers::Exact(LogicalModifiers::NONE),
    }
}

fn in_context(binding: ShortcutBinding, context: Context) -> ShortcutBindingClaim {
    ShortcutBindingClaim {
        binding,
        contexts: vec![context],
        presentation: super::super::model::ShortcutBindingPresentation::DispatchOnly,
    }
}

#[test]
fn duplicate_effective_binding_and_alias_collision_fail_deterministically() {
    let mut descriptors = descriptors();
    let cut = descriptors
        .iter_mut()
        .find(|item| item.action == Action::Cut)
        .expect("cut");
    cut.portable_aliases
        .push(in_context(plain('y'), Context::Board));
    assert_eq!(
        validate_descriptors(&descriptors, ShortcutPlatform::Portable),
        Err(Error::DuplicateBinding {
            context: Context::Board,
            first: Action::Copy,
            second: Action::Cut,
        })
    );
}

#[test]
fn duplicate_context_is_rejected() {
    let mut descriptors = descriptors();
    let close = descriptors
        .iter_mut()
        .find(|item| item.action == Action::Close)
        .expect("close");
    close.contexts.push(Context::Help);
    assert_eq!(
        validate_descriptors(&descriptors, ShortcutPlatform::Portable),
        Err(Error::DuplicateContext {
            action: Action::Close,
            context: Context::Help
        })
    );
}

#[test]
fn invalid_modifier_and_printable_text_theft_are_rejected() {
    let mut invalid_modifier = descriptors();
    invalid_modifier
        .iter_mut()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .portable_defaults
        .push(in_context(
            ShortcutBinding {
                key: LogicalKey::Character('c'),
                modifiers: ShortcutModifiers::Primary,
            },
            Context::Compose,
        ));
    assert_eq!(
        validate_descriptors(&invalid_modifier, ShortcutPlatform::Portable),
        Err(Error::InvalidModifier {
            action: Action::Copy
        })
    );

    let mut text_theft = descriptors();
    text_theft
        .iter_mut()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .portable_defaults
        .push(in_context(plain('c'), Context::Compose));
    assert_eq!(
        validate_descriptors(&text_theft, ShortcutPlatform::Portable),
        Err(Error::TextInputTheft {
            action: Action::Copy,
            context: Context::Compose
        })
    );

    let mut shifted_theft = descriptors();
    shifted_theft
        .iter_mut()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .portable_defaults
        .push(in_context(
            ShortcutBinding {
                key: LogicalKey::Character('?'),
                modifiers: ShortcutModifiers::Exact(LogicalModifiers::SHIFT),
            },
            Context::Compose,
        ));
    assert_eq!(
        validate_descriptors(&shifted_theft, ShortcutPlatform::Portable),
        Err(Error::TextInputTheft {
            action: Action::Copy,
            context: Context::Compose,
        })
    );
}

#[test]
fn every_text_owner_rejects_plain_and_shifted_printable_theft() {
    let text_contexts = [
        Context::Compose,
        Context::Edit,
        Context::Commands,
        Context::Search,
        Context::Invocation,
        Context::InvocationQuery,
        Context::Transfer,
        Context::Browser,
        Context::BrowserQuery,
        Context::Rename,
        Context::BrowserRename,
    ];
    for context in text_contexts {
        for (character, modifiers) in [
            ('c', LogicalModifiers::NONE),
            ('?', LogicalModifiers::SHIFT),
        ] {
            let mut text_theft = descriptors();
            text_theft
                .iter_mut()
                .find(|item| item.action == Action::Copy)
                .expect("copy")
                .portable_defaults
                .push(in_context(
                    ShortcutBinding {
                        key: LogicalKey::Character(character),
                        modifiers: ShortcutModifiers::Exact(modifiers),
                    },
                    context,
                ));
            assert_eq!(
                validate_descriptors(&text_theft, ShortcutPlatform::Portable),
                Err(Error::TextInputTheft {
                    action: Action::Copy,
                    context,
                }),
                "text owner {context:?} accepted {character:?} with {modifiers:?}",
            );
        }
    }
}

#[test]
fn invariant_escape_and_recovery_reachability_fail_closed() {
    let mut escape = descriptors();
    escape
        .iter_mut()
        .find(|item| item.action == Action::Close)
        .expect("close")
        .portable_defaults
        .clear();
    assert!(matches!(
        validate_descriptors(&escape, ShortcutPlatform::Portable),
        Err(Error::InvariantEscapeLoss { .. })
    ));

    let mut recovery = descriptors();
    let retry = recovery
        .iter_mut()
        .find(|item| item.action == Action::RetryStorage)
        .expect("retry");
    retry.contexts.clear();
    retry.commands = None;
    retry.footer = None;
    assert_eq!(
        validate_descriptors(&recovery, ShortcutPlatform::Portable),
        Err(Error::UnreachableRecovery(Action::RetryStorage))
    );
}

#[test]
fn stale_commands_missing_descriptor_and_diagnostics_identity_are_rejected() {
    let mut commands = descriptors();
    commands
        .iter_mut()
        .find(|item| item.action == Action::New)
        .expect("new")
        .commands = None;
    assert_eq!(
        validate_descriptors(&commands, ShortcutPlatform::Portable),
        Err(Error::StaleCommandsReference(Action::New))
    );

    let mut stale = descriptors();
    stale.retain(|item| item.action != Action::New);
    assert_eq!(
        validate_descriptors(&stale, ShortcutPlatform::Portable),
        Err(Error::MissingDescriptor(Action::New))
    );

    let mut diagnostics = descriptors();
    diagnostics
        .iter_mut()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .diagnostics = "";
    assert_eq!(
        validate_descriptors(&diagnostics, ShortcutPlatform::Portable),
        Err(Error::MissingDiagnostics(Action::Copy))
    );

    let mut duplicate_descriptor = descriptors();
    let duplicate = duplicate_descriptor
        .iter()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .clone();
    duplicate_descriptor.push(duplicate);
    assert_eq!(
        validate_descriptors(&duplicate_descriptor, ShortcutPlatform::Portable),
        Err(Error::DuplicateDescriptor(Action::Copy))
    );

    let mut duplicate_diagnostics = descriptors();
    duplicate_diagnostics
        .iter_mut()
        .find(|item| item.action == Action::Cut)
        .expect("cut")
        .diagnostics = Action::Copy.diagnostics_id();
    assert_eq!(
        validate_descriptors(&duplicate_diagnostics, ShortcutPlatform::Portable),
        Err(Error::DuplicateDiagnostics(Action::Copy.diagnostics_id()))
    );

    let mut help = descriptors();
    help.iter_mut()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .help
        .clear();
    assert_eq!(
        validate_descriptors(&help, ShortcutPlatform::Portable),
        Err(Error::StaleHelpReference(Action::Copy))
    );

    let mut footer = descriptors();
    footer
        .iter_mut()
        .find(|item| item.action == Action::Copy)
        .expect("copy")
        .footer = None;
    assert_eq!(
        validate_descriptors(&footer, ShortcutPlatform::Portable),
        Err(Error::StaleFooterReference(Action::Copy))
    );
}
