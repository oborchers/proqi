//! Reserved Primary suffixes used only during explicit legacy translation.

use super::model::ShortcutActionId as Action;
use super::{
    inventory,
    model::{ShortcutBindingPresentation, ShortcutDescriptor, ShortcutModifiers},
};
use crate::ui::{KeyBindings, LogicalKey, LogicalModifiers};
use std::sync::OnceLock;

pub(crate) fn reserved_unshifted_character(character: char) -> bool {
    unshifted_primary_characters().any(|(_, candidate)| candidate.eq_ignore_ascii_case(&character))
}

pub(crate) fn reserved_shifted_configuration_suffix(character: char) -> bool {
    unshifted_primary_characters().any(|(action, candidate)| {
        action != Action::DeleteLogicalLine && candidate.eq_ignore_ascii_case(&character)
    })
}

fn unshifted_primary_characters() -> impl Iterator<Item = (Action, char)> {
    legacy_descriptors()
        .iter()
        .filter(|descriptor| {
            // The new reflow alias yields to previously valid explicit legacy bindings.
            descriptor.action != Action::ReflowThought
        })
        .flat_map(|descriptor| {
            platform_defaults(descriptor, false)
                .iter()
                .filter_map(move |claim| {
                    if !matches!(claim.presentation, ShortcutBindingPresentation::Primary) {
                        return None;
                    }
                    let ShortcutModifiers::Exact(modifiers) = claim.binding.modifiers else {
                        return None;
                    };
                    match claim.binding.key {
                        LogicalKey::Character(character)
                            if super::ShortcutPlatform::Portable.is_primary(modifiers)
                                && !modifiers.contains(LogicalModifiers::SHIFT) =>
                        {
                            Some((descriptor.action, character))
                        }
                        _ => None,
                    }
                })
        })
}

fn legacy_descriptors() -> &'static [ShortcutDescriptor] {
    static DESCRIPTORS: OnceLock<Vec<ShortcutDescriptor>> = OnceLock::new();
    DESCRIPTORS.get_or_init(|| inventory::descriptors(&KeyBindings::default()))
}

fn platform_defaults(
    descriptor: &ShortcutDescriptor,
    macos: bool,
) -> &[super::model::ShortcutBindingClaim] {
    if macos {
        &descriptor.macos_defaults
    } else {
        &descriptor.portable_defaults
    }
}
