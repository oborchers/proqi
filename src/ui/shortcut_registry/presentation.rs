//! Registry-owned labels shared by Help, footer controls, and validation.

mod footer;
mod help;

pub(crate) use footer::footer_projection;
pub(crate) use help::{HelpItem, help_items};

use std::sync::OnceLock;

use crate::ui::{KeyBindings, LogicalKey, LogicalModifiers, settings};

use super::{
    inventory,
    model::{
        ShortcutActionId as Action, ShortcutBindingPresentation, ShortcutDescriptor,
        ShortcutModifiers,
    },
};

pub(crate) fn primary_label(action: Action) -> String {
    label(action, false)
}

pub(crate) fn shifted_primary_label(action: Action) -> String {
    label(action, true)
}

pub(crate) fn redo_label() -> String {
    format!(
        "{}/{}",
        shifted_primary_label(Action::Redo),
        primary_label(Action::Redo)
    )
}

pub(crate) fn board_label(action: Action, keys: &KeyBindings) -> String {
    let primary = canonical_label(action);
    let fallbacks = board_fallbacks(action, keys);
    if fallbacks.is_empty() {
        primary
    } else {
        format!("{primary}/{}", fallback_label(&fallbacks))
    }
}

pub(crate) fn board_control_label(action: Action, keys: &KeyBindings, compact: bool) -> String {
    let fallbacks = board_fallbacks(action, keys);
    if fallbacks.is_empty() {
        return canonical_label(action);
    }
    if compact {
        fallback_label(&fallbacks)
    } else {
        board_label(action, keys)
    }
}

pub(crate) fn canonical_label(action: Action) -> String {
    primary_suffix(action, None, true)
        .or_else(|| primary_suffix(action, Some(false), false))
        .or_else(|| primary_suffix(action, Some(true), false))
        .map_or_else(|| "Primary".to_owned(), |suffix| primary(&suffix))
}

pub(crate) fn reserved_unshifted_character(character: char) -> bool {
    unshifted_primary_characters().any(|(_, candidate)| candidate.eq_ignore_ascii_case(&character))
}

pub(crate) fn reserved_shifted_configuration_suffix(character: char) -> bool {
    unshifted_primary_characters().any(|(action, candidate)| {
        action != Action::DeleteLogicalLine && candidate.eq_ignore_ascii_case(&character)
    })
}

fn label(action: Action, shifted: bool) -> String {
    primary_suffix(action, Some(shifted), false)
        .map_or_else(|| "Primary".to_owned(), |suffix| primary(&suffix))
}

fn primary_suffix(action: Action, shifted: Option<bool>, canonical: bool) -> Option<String> {
    let (key, binding_shifted) = primary_binding(action, shifted, canonical)?;
    let key = match key {
        LogicalKey::Character(character) => character.to_ascii_uppercase().to_string(),
        LogicalKey::Enter => "Enter".to_owned(),
        _ => return None,
    };
    Some(if binding_shifted {
        format!("Shift+{key}")
    } else {
        key
    })
}

fn primary_binding(
    action: Action,
    shifted: Option<bool>,
    canonical: bool,
) -> Option<(LogicalKey, bool)> {
    let macos = cfg!(target_os = "macos");
    let descriptor = canonical_descriptors()
        .iter()
        .find(|descriptor| descriptor.action == action)?;
    platform_defaults(descriptor, macos)
        .iter()
        .find_map(|claim| {
            let ShortcutBindingPresentation::Primary {
                canonical: is_canonical,
            } = claim.presentation
            else {
                return None;
            };
            let ShortcutModifiers::Exact(modifiers) = claim.binding.modifiers else {
                return None;
            };
            let binding_shifted = modifiers.contains(LogicalModifiers::SHIFT);
            (super::inventory::bindings::is_primary(modifiers, macos)
                && shifted.is_none_or(|expected| expected == binding_shifted)
                && (!canonical || is_canonical))
                .then_some((claim.binding.key, binding_shifted))
        })
}

fn unshifted_primary_characters() -> impl Iterator<Item = (Action, char)> {
    canonical_descriptors().iter().flat_map(|descriptor| {
        platform_defaults(descriptor, false)
            .iter()
            .filter_map(move |claim| {
                if !matches!(
                    claim.presentation,
                    ShortcutBindingPresentation::Primary { .. }
                ) {
                    return None;
                }
                let ShortcutModifiers::Exact(modifiers) = claim.binding.modifiers else {
                    return None;
                };
                match claim.binding.key {
                    LogicalKey::Character(character)
                        if super::inventory::bindings::is_primary(modifiers, false)
                            && !modifiers.contains(LogicalModifiers::SHIFT) =>
                    {
                        Some((descriptor.action, character))
                    }
                    _ => None,
                }
            })
    })
}

fn canonical_descriptors() -> &'static [ShortcutDescriptor] {
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

fn primary(suffix: &str) -> String {
    settings::primary_key_label(suffix)
}

fn board_fallbacks(action: Action, keys: &KeyBindings) -> Vec<char> {
    super::context_policy::effective_board_bindings(keys)
        .into_iter()
        .filter_map(|(key, mapped)| (mapped == action).then_some(key))
        .collect()
}

fn fallback_label(fallbacks: &[char]) -> String {
    fallbacks
        .iter()
        .map(|character| settings::key_label(*character))
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_primary_projection_is_derived_from_an_effective_descriptor_default() {
        for descriptor in canonical_descriptors() {
            for shifted in [false, true] {
                assert!(
                    primary_binding(descriptor.action, Some(shifted), false).is_none()
                        || label(descriptor.action, shifted) != "Primary"
                );
            }
        }
    }

    #[test]
    fn paste_fallbacks_report_only_effective_aliases() {
        let keys = KeyBindings {
            paste: 'g',
            submit_keep: 'G',
            ..KeyBindings::default()
        };
        assert_eq!(board_fallbacks(Action::PasteExact, &keys), vec!['g']);
        assert!(board_fallbacks(Action::PasteReflow, &keys).is_empty());
    }
}
