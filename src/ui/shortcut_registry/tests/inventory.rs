use std::collections::BTreeSet;

use crate::ui::{
    KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack, ShortcutModifiers,
};

use super::super::{ShortcutPlatform, ShortcutRegistry, inventory};
use super::stroke;

#[test]
fn every_commands_entry_has_one_matching_registry_descriptor() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    assert_eq!(Action::COMMANDS.len(), 51);
    for (order, (action, label)) in Action::COMMANDS.into_iter().enumerate() {
        let descriptor = registry.descriptor(action).expect("Commands descriptor");
        assert_eq!(
            descriptor.commands,
            Some(inventory::metadata::command_metadata(action, order, label))
        );
        assert!(descriptor.contexts.contains(&Context::Commands));
    }
}

#[test]
fn descriptor_diagnostics_are_complete_unique_and_content_free() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    let mut identities = BTreeSet::new();
    for descriptor in registry.descriptors() {
        assert_eq!(descriptor.diagnostics, descriptor.action.diagnostics_id());
        assert!(!descriptor.diagnostics.is_empty());
        assert!(identities.insert(descriptor.diagnostics));
    }
}

#[test]
fn every_discovered_keyboard_owner_is_qualified_by_a_descriptor() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    let covered = registry
        .descriptors()
        .iter()
        .flat_map(|descriptor| descriptor.contexts.iter().copied())
        .collect::<BTreeSet<_>>();
    let expected = BTreeSet::from([
        Context::Board,
        Context::Compose,
        Context::Edit,
        Context::Help,
        Context::Commands,
        Context::Search,
        Context::Invocation,
        Context::InvocationQuery,
        Context::Transfer,
        Context::Browser,
        Context::BrowserQuery,
        Context::Rename,
        Context::BrowserRename,
        Context::Update,
        Context::Screenshot,
        Context::Recovery,
        Context::Direction,
        Context::ReleaseHighlights,
        Context::InsertionBoundary,
    ]);
    assert_eq!(covered, expected);
}

#[test]
fn help_footer_and_commands_references_are_all_typed_action_identities() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    assert!(
        registry
            .descriptors()
            .iter()
            .any(|descriptor| !descriptor.help.is_empty())
    );
    assert!(
        registry
            .descriptors()
            .iter()
            .any(|descriptor| descriptor.footer.is_some())
    );
    for descriptor in registry.descriptors().iter().filter(|descriptor| {
        !descriptor.help.is_empty() || descriptor.footer.is_some() || descriptor.commands.is_some()
    }) {
        assert!(registry.descriptor(descriptor.action).is_some());
    }
}

#[test]
fn current_board_and_editor_configuration_is_projected_as_context_owned_aliases() {
    let keys = KeyBindings {
        new: 'g',
        transform: 'b',
        delete_sentence: 'F',
        select_visual_row_start: 'M',
        select_visual_row_end: 'R',
        ..KeyBindings::default()
    };
    let registry = ShortcutRegistry::resolve(&keys, ShortcutPlatform::Portable)
        .expect("valid configured registry");

    let new = registry.descriptor(Action::New).expect("new");
    assert!(new.portable_aliases.iter().any(|claim| {
        claim.binding.key == LogicalKey::Character('g')
            && claim.contexts == [Context::Board, Context::InsertionBoundary]
    }));
    for (action, key, shifted) in [
        (Action::ContextualTransform, 'b', false),
        (Action::DeleteSentence, 'F', true),
        (Action::ExtendVisualRowStart, 'M', true),
        (Action::ExtendVisualRowEnd, 'R', true),
    ] {
        let descriptor = registry.descriptor(action).expect("configured action");
        let modifiers = LogicalModifiers::CONTROL.union(if shifted {
            LogicalModifiers::SHIFT
        } else {
            LogicalModifiers::NONE
        });
        assert!(descriptor.portable_aliases.iter().any(|claim| {
            claim.binding.key == LogicalKey::Character(key)
                && claim.binding.modifiers == ShortcutModifiers::Exact(modifiers)
                && claim.contexts == [Context::Compose, Context::Edit, Context::Invocation]
        }));
    }
}

#[test]
fn recovery_browser_modal_and_direction_routes_have_owned_bindings() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for (action, context) in [
        (Action::RetryStorage, Context::Recovery),
        (Action::ExportRecovery, Context::Recovery),
        (Action::RenameSession, Context::Browser),
        (Action::BrowserTrash, Context::Browser),
        (Action::ChooseLeft, Context::Direction),
        (Action::ChooseDown, Context::Direction),
        (Action::ChooseUp, Context::Direction),
        (Action::ChooseRight, Context::Direction),
    ] {
        let descriptor = registry.descriptor(action).expect("direct descriptor");
        assert!(
            descriptor
                .macos_defaults
                .iter()
                .any(|claim| claim.contexts.contains(&context))
        );
    }
}

#[test]
fn every_effective_descriptor_claim_dispatches_to_its_declared_action() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for descriptor in registry.descriptors() {
            assert_descriptor_claims(&registry, descriptor, platform);
        }
    }
}

fn assert_descriptor_claims(
    registry: &ShortcutRegistry,
    descriptor: &crate::ui::ShortcutDescriptor,
    platform: ShortcutPlatform,
) {
    let (defaults, aliases) = match platform {
        ShortcutPlatform::MacOs => (&descriptor.macos_defaults, &descriptor.macos_aliases),
        ShortcutPlatform::Portable => (&descriptor.portable_defaults, &descriptor.portable_aliases),
    };
    for claim in defaults.iter().chain(aliases) {
        let ShortcutModifiers::Exact(modifiers) = claim.binding.modifiers else {
            panic!("effective descriptor retained an unresolved modifier policy");
        };
        for context in claim.contexts.iter().copied() {
            let resolved = registry
                .dispatch(
                    &ShortcutContextStack::new([context]),
                    stroke(claim.binding.key, modifiers),
                )
                .expect("declared binding dispatches");
            assert_eq!(
                resolved.action,
                Some(descriptor.action),
                "{platform:?} {context:?} {:?}",
                claim.binding,
            );
        }
    }
}
