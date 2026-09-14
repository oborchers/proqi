use super::*;
use crate::ui::shortcut_registry::{
    CommandApplicability, CommandCategory, CommandDiscoverability, CommandRelevance, CommandScope,
    ShortcutActionId,
};

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
        Action::ReflowThought,
        Action::SubmitKeep,
    ] {
        let metadata = registry
            .descriptor(contextual)
            .and_then(|descriptor| descriptor.commands)
            .expect("contextual Commands descriptor");
        assert_eq!(metadata.scope, CommandScope::Contextual);
    }
    for query_history in [Action::Undo, Action::Redo] {
        assert_eq!(
            registry
                .descriptor(query_history)
                .and_then(|descriptor| descriptor.commands)
                .expect("query history Commands descriptor")
                .scope,
            CommandScope::Commands
        );
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
fn commands_metadata_owns_reflow_relevance_and_contextual_transform_bindings() {
    let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
        .expect("valid registry");
    let commands = |action| {
        registry
            .descriptor(action)
            .and_then(|descriptor| descriptor.commands)
            .expect("Commands metadata")
    };

    let reflow = commands(Action::ReflowThought);
    assert_eq!(reflow.relevance, CommandRelevance::FocusedThought(25));
    assert_eq!(reflow.shortcut_owner, ShortcutActionId::ReflowThought);
    for action in [
        Action::SplitThought,
        Action::ExtractSelection,
        Action::MergeThoughts,
    ] {
        assert_eq!(
            commands(action).shortcut_owner,
            ShortcutActionId::ContextualTransform
        );
    }
    for action in [Action::RenameSession, Action::CopyResume] {
        assert_eq!(commands(action).relevance, CommandRelevance::Never);
    }
}
