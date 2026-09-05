//! Typed load-time validation of the effective shortcut graph.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::ui::{LogicalKey, LogicalModifiers, settings::KeyBindings};

use super::{
    dispatch::ShortcutPlatform,
    inventory,
    model::{
        ShortcutActionId as Action, ShortcutBinding, ShortcutContext as Context,
        ShortcutDescriptor, ShortcutModifiers,
    },
};

/// Deterministic configuration failure reported before terminal setup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutRegistryError {
    InvalidOverride(&'static str),
    DuplicateBinding {
        context: Context,
        first: Action,
        second: Action,
    },
    DuplicateContext {
        action: Action,
        context: Context,
    },
    InvalidModifier {
        action: Action,
    },
    TextInputTheft {
        action: Action,
        context: Context,
    },
    InvariantEscapeLoss {
        context: Context,
    },
    UnreachableRecovery(Action),
    MissingDescriptor(Action),
    DuplicateDescriptor(Action),
    MissingDiagnostics(Action),
    DuplicateDiagnostics(&'static str),
    StaleCommandsReference(Action),
    StaleHelpReference(Action),
    StaleFooterReference(Action),
}

impl fmt::Display for ShortcutRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOverride(message) => formatter.write_str(message),
            Self::DuplicateBinding {
                context,
                first,
                second,
            } => write!(
                formatter,
                "shortcut actions {} and {} claim the same binding in {context:?}",
                first.diagnostics_id(),
                second.diagnostics_id()
            ),
            Self::DuplicateContext { action, context } => write!(
                formatter,
                "shortcut action {} repeats context {context:?}",
                action.diagnostics_id()
            ),
            Self::InvalidModifier { action } => write!(
                formatter,
                "shortcut action {} has an ineligible modifier combination",
                action.diagnostics_id()
            ),
            Self::TextInputTheft { action, context } => write!(
                formatter,
                "shortcut action {} would steal printable input in {context:?}",
                action.diagnostics_id()
            ),
            Self::InvariantEscapeLoss { context } => {
                write!(
                    formatter,
                    "Escape must remain the close or cancel route in {context:?}"
                )
            }
            Self::UnreachableRecovery(action) => write!(
                formatter,
                "required recovery action {} is unreachable",
                action.diagnostics_id()
            ),
            Self::MissingDescriptor(action) => write!(
                formatter,
                "shortcut action {} has no registry descriptor",
                action.diagnostics_id()
            ),
            Self::DuplicateDescriptor(action) => write!(
                formatter,
                "shortcut action {} has duplicate registry descriptors",
                action.diagnostics_id()
            ),
            Self::MissingDiagnostics(action) => write!(
                formatter,
                "shortcut action {action:?} has no diagnostics identity"
            ),
            Self::DuplicateDiagnostics(identity) => {
                write!(
                    formatter,
                    "shortcut diagnostics identity {identity} is duplicated"
                )
            }
            Self::StaleCommandsReference(action) => write!(
                formatter,
                "Commands references missing shortcut action {}",
                action.diagnostics_id()
            ),
            Self::StaleHelpReference(action) => write!(
                formatter,
                "Help references missing shortcut action {}",
                action.diagnostics_id()
            ),
            Self::StaleFooterReference(action) => write!(
                formatter,
                "footer references missing shortcut action {}",
                action.diagnostics_id()
            ),
        }
    }
}

impl std::error::Error for ShortcutRegistryError {}

pub(super) fn validate_registry(
    keys: &KeyBindings,
    platform: ShortcutPlatform,
) -> Result<(), ShortcutRegistryError> {
    keys.validate()
        .map_err(ShortcutRegistryError::InvalidOverride)?;
    let descriptors = inventory::descriptors(keys);
    validate_descriptors(&descriptors, platform)
}

pub(super) fn validate_descriptors(
    descriptors: &[ShortcutDescriptor],
    platform: ShortcutPlatform,
) -> Result<(), ShortcutRegistryError> {
    validate_descriptor_identities(descriptors)?;
    validate_contexts(descriptors)?;
    validate_binding_claims(descriptors, platform)?;
    validate_escape(descriptors, platform)?;
    validate_recovery(descriptors, platform)?;
    validate_presentation_references(descriptors)?;
    validate_commands(descriptors)
}

fn validate_descriptor_identities(
    descriptors: &[ShortcutDescriptor],
) -> Result<(), ShortcutRegistryError> {
    let mut actions = BTreeSet::new();
    let mut diagnostics = BTreeSet::new();
    for descriptor in descriptors {
        if !actions.insert(descriptor.action) {
            return Err(ShortcutRegistryError::DuplicateDescriptor(
                descriptor.action,
            ));
        }
        if descriptor.diagnostics.is_empty() {
            return Err(ShortcutRegistryError::MissingDiagnostics(descriptor.action));
        }
        if !diagnostics.insert(descriptor.diagnostics) {
            return Err(ShortcutRegistryError::DuplicateDiagnostics(
                descriptor.diagnostics,
            ));
        }
    }
    for action in inventory::DIRECT_ACTIONS {
        if !actions.contains(action) {
            return Err(ShortcutRegistryError::MissingDescriptor(*action));
        }
    }
    Ok(())
}

fn validate_contexts(descriptors: &[ShortcutDescriptor]) -> Result<(), ShortcutRegistryError> {
    for descriptor in descriptors {
        let mut contexts = BTreeSet::new();
        for context in &descriptor.contexts {
            if !contexts.insert(*context) {
                return Err(ShortcutRegistryError::DuplicateContext {
                    action: descriptor.action,
                    context: *context,
                });
            }
        }
    }
    Ok(())
}

fn validate_binding_claims(
    descriptors: &[ShortcutDescriptor],
    platform: ShortcutPlatform,
) -> Result<(), ShortcutRegistryError> {
    let mut claims = BTreeMap::new();
    for descriptor in descriptors {
        let (defaults, aliases) = match platform {
            ShortcutPlatform::MacOs => (&descriptor.macos_defaults, &descriptor.macos_aliases),
            ShortcutPlatform::Portable => {
                (&descriptor.portable_defaults, &descriptor.portable_aliases)
            }
        };
        for binding_claim in defaults.iter().chain(aliases) {
            validate_modifier(descriptor.action, &binding_claim.binding)?;
            for context in &binding_claim.contexts {
                validate_text_safety(descriptor.action, *context, &binding_claim.binding)?;
                claim(
                    &mut claims,
                    *context,
                    binding_claim.binding,
                    descriptor.action,
                )?;
            }
        }
    }
    Ok(())
}

fn claim(
    claims: &mut BTreeMap<(Context, ShortcutBinding), Action>,
    context: Context,
    binding: ShortcutBinding,
    action: Action,
) -> Result<(), ShortcutRegistryError> {
    if let Some(first) = claims.insert((context, binding), action)
        && first != action
    {
        return Err(ShortcutRegistryError::DuplicateBinding {
            context,
            first,
            second: action,
        });
    }
    Ok(())
}

fn validate_modifier(
    action: Action,
    binding: &ShortcutBinding,
) -> Result<(), ShortcutRegistryError> {
    match binding.modifiers {
        ShortcutModifiers::Exact(_) => Ok(()),
        ShortcutModifiers::Primary
        | ShortcutModifiers::PrimaryShift
        | ShortcutModifiers::Contextual => Err(ShortcutRegistryError::InvalidModifier { action }),
    }
}

fn validate_text_safety(
    action: Action,
    context: Context,
    binding: &ShortcutBinding,
) -> Result<(), ShortcutRegistryError> {
    let printable =
        matches!(binding.key, LogicalKey::Character(character) if !character.is_control());
    let unmodified = matches!(
        binding.modifiers,
        ShortcutModifiers::Exact(modifiers)
            if modifiers.is_empty() || modifiers == LogicalModifiers::SHIFT
    );
    let established_browser_management = context == Context::Browser
        && matches!(action, Action::RenameSession | Action::BrowserTrash);
    if is_text_context(context) && printable && unmodified && !established_browser_management {
        return Err(ShortcutRegistryError::TextInputTheft { action, context });
    }
    Ok(())
}

const fn is_text_context(context: Context) -> bool {
    matches!(
        context,
        Context::Compose
            | Context::Edit
            | Context::Commands
            | Context::Search
            | Context::Invocation
            | Context::InvocationQuery
            | Context::Transfer
            | Context::Browser
            | Context::BrowserQuery
            | Context::Rename
            | Context::BrowserRename
    )
}

fn validate_escape(
    descriptors: &[ShortcutDescriptor],
    platform: ShortcutPlatform,
) -> Result<(), ShortcutRegistryError> {
    let Some(close) = descriptors
        .iter()
        .find(|descriptor| descriptor.action == Action::Close)
    else {
        return Err(ShortcutRegistryError::MissingDescriptor(Action::Close));
    };
    let bindings = match platform {
        ShortcutPlatform::MacOs => &close.macos_defaults,
        ShortcutPlatform::Portable => &close.portable_defaults,
    };
    for context in inventory::ESCAPE_CONTEXTS.iter().copied() {
        let owns_escape = close.contexts.contains(&context)
            && bindings.iter().any(|claim| {
                claim.contexts.contains(&context)
                    && claim.binding.key == LogicalKey::Escape
                    && claim.binding.modifiers == ShortcutModifiers::Exact(LogicalModifiers::NONE)
            });
        if !owns_escape {
            return Err(ShortcutRegistryError::InvariantEscapeLoss { context });
        }
    }
    Ok(())
}

fn validate_recovery(
    descriptors: &[ShortcutDescriptor],
    platform: ShortcutPlatform,
) -> Result<(), ShortcutRegistryError> {
    for action in [Action::Quit, Action::RetryStorage, Action::ExportRecovery] {
        let reachable = descriptors
            .iter()
            .find(|descriptor| descriptor.action == action)
            .is_some_and(|descriptor| {
                let (defaults, aliases) = match platform {
                    ShortcutPlatform::MacOs => {
                        (&descriptor.macos_defaults, &descriptor.macos_aliases)
                    }
                    ShortcutPlatform::Portable => {
                        (&descriptor.portable_defaults, &descriptor.portable_aliases)
                    }
                };
                descriptor.contexts.contains(&Context::Recovery)
                    && defaults
                        .iter()
                        .chain(aliases)
                        .any(|claim| claim.contexts.contains(&Context::Recovery))
            });
        if !reachable {
            return Err(ShortcutRegistryError::UnreachableRecovery(action));
        }
    }
    Ok(())
}

fn validate_commands(descriptors: &[ShortcutDescriptor]) -> Result<(), ShortcutRegistryError> {
    for (order, (action, label)) in Action::COMMANDS.into_iter().enumerate() {
        let expected = inventory::metadata::command_metadata(action, order, label);
        let valid = descriptors
            .iter()
            .any(|descriptor| descriptor.action == action && descriptor.commands == Some(expected));
        if !valid {
            return Err(ShortcutRegistryError::StaleCommandsReference(action));
        }
    }
    Ok(())
}

fn validate_presentation_references(
    descriptors: &[ShortcutDescriptor],
) -> Result<(), ShortcutRegistryError> {
    for descriptor in descriptors {
        if descriptor.help != inventory::metadata::help_metadata(descriptor.action) {
            return Err(ShortcutRegistryError::StaleHelpReference(descriptor.action));
        }
        if descriptor.footer != inventory::metadata::footer_metadata(descriptor.action) {
            return Err(ShortcutRegistryError::StaleFooterReference(
                descriptor.action,
            ));
        }
    }
    Ok(())
}
