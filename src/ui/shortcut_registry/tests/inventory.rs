use std::collections::BTreeSet;

use crate::ui::{
    KeyBindings, LogicalKey, LogicalModifiers, ShortcutActionId as Action,
    ShortcutContext as Context, ShortcutContextStack, ShortcutModifiers,
};

use super::super::{ShortcutPlatform, ShortcutRegistry, inventory};
use super::stroke;

const SHORTCUTS_DOCUMENT: &str = include_str!("../../../../context/SHORTCUTS.md");

#[test]
fn duplicate_presentation_includes_the_terminal_safe_board_alias() {
    for (platform, primary) in [
        (ShortcutPlatform::MacOs, "Cmd+D"),
        (ShortcutPlatform::Portable, "Ctrl+D"),
    ] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        let labels = registry.labels(Context::Board, Action::Duplicate);
        assert!(
            labels.contains(&primary.to_owned()),
            "{platform:?}: {labels:?}"
        );
        assert!(
            labels.contains(&"Shift+D".to_owned()),
            "{platform:?}: {labels:?}"
        );
        assert_eq!(
            registry.action_label(Context::Board, Action::Duplicate, true),
            primary
        );
    }
}

#[test]
fn macos_reorder_presentation_includes_the_terminal_safe_option_aliases() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
        .expect("valid registry");
    for (action, expected, hidden) in [
        (Action::MoveUp, "Option+Shift+↑", "Option+Shift+K"),
        (Action::MoveDown, "Option+Shift+↓", "Option+Shift+J"),
    ] {
        let labels = registry.labels(Context::Board, action);
        assert!(
            labels.contains(&expected.to_owned()),
            "{action:?}: {labels:?}"
        );
        assert!(
            !labels.contains(&hidden.to_owned()),
            "{action:?}: {labels:?}"
        );
    }
    assert_eq!(
        registry.compact_help_label(Context::Board, &[Action::MoveDown, Action::MoveUp]),
        "Option+Shift+↓/↑"
    );
}

#[test]
fn compact_help_chooses_one_shortest_alias_per_action() {
    let registry = ShortcutRegistry::from_toml(
        "schema_version=1\n[bindings.board]\n\"thought.delete\"=[{key='F5'},{key='F6'},{key='F7'}]",
    )
    .expect("valid multi-alias registry");
    assert_eq!(
        registry.compact_help_label(Context::Board, &[Action::Delete]),
        "F5"
    );
}

#[test]
fn every_commands_entry_has_one_matching_registry_descriptor() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    assert_eq!(Action::COMMANDS.len(), 57);
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
fn commands_descriptors_own_independent_disclosure_dimensions() {
    use super::super::{
        CommandApplicability, CommandCategory, CommandDiscoverability, CommandRelevance,
        CommandScope,
    };

    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    let commands = registry.commands();
    assert!(commands.iter().all(|(_, metadata, _)| {
        metadata.discoverability == CommandDiscoverability::Discoverable
    }));

    let retry = registry
        .descriptor(Action::RetryStorage)
        .and_then(|descriptor| descriptor.commands)
        .expect("retry descriptor");
    assert_eq!(retry.applicability, CommandApplicability::RetryStorage);
    assert_eq!(retry.relevance, CommandRelevance::StorageRecovery(0));
    assert_eq!(retry.category, CommandCategory::ApplicationAndRecovery);
    assert_eq!(
        registry
            .descriptor(Action::Copy)
            .and_then(|descriptor| descriptor.commands)
            .expect("copy descriptor")
            .applicability,
        CommandApplicability::Copy
    );
    assert_eq!(
        registry
            .descriptor(Action::Cut)
            .and_then(|descriptor| descriptor.commands)
            .expect("cut descriptor")
            .applicability,
        CommandApplicability::Cut
    );

    for contextual in [
        Action::Copy,
        Action::Cut,
        Action::PasteExact,
        Action::Undo,
        Action::SubmitKeep,
    ] {
        let metadata = registry
            .descriptor(contextual)
            .and_then(|descriptor| descriptor.commands)
            .expect("contextual Commands descriptor");
        assert_eq!(metadata.scope, CommandScope::Contextual);
    }
    assert_eq!(
        registry
            .descriptor(Action::SelectAll)
            .and_then(|descriptor| descriptor.commands)
            .expect("Select all descriptor")
            .scope,
        CommandScope::Selection
    );

    for destructive in [
        Action::Delete,
        Action::Cut,
        Action::SubmitRemove,
        Action::SendSessionRemove,
    ] {
        let metadata = registry
            .descriptor(destructive)
            .and_then(|descriptor| descriptor.commands)
            .expect("destructive Commands descriptor");
        assert_eq!(metadata.relevance, CommandRelevance::Never);
    }
}

#[test]
fn shortcut_inventory_document_tracks_contexts_and_commands_count() {
    let context_rows = SHORTCUTS_DOCUMENT
        .lines()
        .skip_while(|line| *line != "| Context | Current owner | Text reservation |")
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .count();
    assert_eq!(
        context_rows,
        inventory::bindings::vocabulary::KEYBOARD_CONTEXTS.len(),
        "inventory table must not contain missing or stale context rows"
    );
    for context in inventory::bindings::vocabulary::KEYBOARD_CONTEXTS {
        let row_prefix = format!("| {context:?} |");
        assert_eq!(
            SHORTCUTS_DOCUMENT.matches(&row_prefix).count(),
            1,
            "{context:?} must have exactly one inventory table row"
        );
    }
    let commands_count = format!("all {} current Commands actions", Action::COMMANDS.len());
    assert!(
        SHORTCUTS_DOCUMENT.contains(&commands_count),
        "inventory must derive its Commands count from the typed action owner"
    );
}

#[test]
fn fixed_recovery_bindings_are_descriptor_owned_and_dispatchable() {
    for platform in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), platform).expect("valid registry");
        for action in [Action::RetryStorage, Action::ExportRecovery] {
            let character = inventory::fixed_character_binding(action, Context::Recovery)
                .expect("recovery action has a fixed registry binding");
            let descriptor = registry.descriptor(action).expect("recovery descriptor");
            let defaults = match platform {
                ShortcutPlatform::MacOs => &descriptor.macos_defaults,
                ShortcutPlatform::Portable => &descriptor.portable_defaults,
            };
            assert!(defaults.iter().any(|claim| {
                claim.binding.key == LogicalKey::Character(character)
                    && claim.contexts == [Context::Recovery]
            }));
            let resolved = registry
                .dispatch(
                    &ShortcutContextStack::new([Context::Recovery]),
                    stroke(LogicalKey::Character(character), LogicalModifiers::NONE),
                )
                .expect("fixed recovery binding dispatches");
            assert_eq!(resolved.action, Some(action));
        }
    }
}

#[test]
fn configured_commands_binding_opens_fresh_commands_from_recovery() {
    let keys = KeyBindings {
        commands: 'τ',
        ..KeyBindings::default()
    };
    let registry =
        ShortcutRegistry::resolve(&keys, ShortcutPlatform::Portable).expect("valid registry");
    let resolved = registry
        .dispatch(
            &ShortcutContextStack::new([Context::Recovery]),
            stroke(LogicalKey::Character('τ'), LogicalModifiers::NONE),
        )
        .expect("configured Commands binding dispatches in Recovery");
    assert_eq!(resolved.action, Some(Action::OpenCommands));
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
        Context::GlobalDeliveryQuery,
        Context::GlobalDeliveryDisposition,
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
