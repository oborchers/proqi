//! Versioned configuration translated into one resolved contextual graph.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::{
    contract::{CONTEXT_NAMES, parse_key},
    dispatch::{ShortcutPlatform, ShortcutRegistry},
    inventory,
    model::{
        ShortcutActionId as Action, ShortcutBinding, ShortcutBindingClaim,
        ShortcutBindingPresentation, ShortcutContext as Context, ShortcutDescriptor,
        ShortcutModifiers,
    },
    validation::{ShortcutRegistryError as Error, validate_descriptors},
};
use crate::ui::{KeyBindings, LogicalKey, LogicalModifiers};

type ContextMaps = BTreeMap<String, BTreeMap<String, Vec<AliasDocument>>>;

/// Configuration-only input. Never retained by a running application.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeymapDocument {
    schema_version: u16,
    #[serde(default)]
    bindings: ContextMaps,
    #[serde(default)]
    macos: ContextMaps,
    #[serde(default)]
    portable: ContextMaps,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AliasDocument {
    key: String,
    #[serde(default)]
    modifiers: Vec<String>,
}

type Overrides = BTreeMap<(Context, Action), Vec<ShortcutBinding>>;

impl KeymapDocument {
    pub(crate) fn resolve(&self, platform: ShortcutPlatform) -> Result<ShortcutRegistry, Error> {
        if self.schema_version != 1 {
            return Err(Error::UnsupportedVersion(self.schema_version));
        }
        let mut descriptors = inventory::descriptors(&KeyBindings::default());
        // Validate both platform maps, including the currently inactive one.
        for candidate in [ShortcutPlatform::MacOs, ShortcutPlatform::Portable] {
            let mut overrides = parse_maps(&self.bindings, candidate, &descriptors)?;
            let platform_maps = if candidate == ShortcutPlatform::MacOs {
                &self.macos
            } else {
                &self.portable
            };
            overrides.extend(parse_maps(platform_maps, candidate, &descriptors)?);
            apply_overrides(&mut descriptors, candidate, &overrides);
            validate_descriptors(&descriptors, candidate)?;
        }
        Ok(ShortcutRegistry::from_descriptors(descriptors, platform))
    }
}

fn parse_maps(
    maps: &ContextMaps,
    platform: ShortcutPlatform,
    descriptors: &[ShortcutDescriptor],
) -> Result<Overrides, Error> {
    let mut result = BTreeMap::new();
    for (context_index, (context_name, actions)) in maps.iter().enumerate() {
        let context = CONTEXT_NAMES
            .iter()
            .find_map(|(context, name)| (*name == context_name).then_some(*context))
            .ok_or(Error::UnknownContext(context_index))?;
        for (action_index, (name, aliases)) in actions.iter().enumerate() {
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.diagnostics == name)
                .ok_or(Error::UnknownAction {
                    context,
                    index: action_index,
                })?;
            let action = descriptor.action;
            if !descriptor.contexts.contains(&context) {
                return Err(Error::UnsupportedContext { context, action });
            }
            let bindings = parse_aliases(aliases, platform, context, action)?;
            result.insert((context, action), bindings);
        }
    }
    Ok(result)
}

fn parse_aliases(
    aliases: &[AliasDocument],
    platform: ShortcutPlatform,
    context: Context,
    action: Action,
) -> Result<Vec<ShortcutBinding>, Error> {
    if aliases.len() > 128 {
        return Err(Error::TooManyAliases { context, action });
    }
    let mut bindings = Vec::new();
    let mut unique = BTreeSet::new();
    for (index, alias) in aliases.iter().enumerate() {
        for binding in parse_alias(alias, platform).ok_or(Error::MalformedAlias {
            context,
            action,
            index,
        })? {
            if !unique.insert(binding) {
                return Err(Error::DuplicateAlias {
                    context,
                    action,
                    index,
                });
            }
            bindings.push(binding);
        }
    }
    Ok(bindings)
}

fn parse_alias(alias: &AliasDocument, platform: ShortcutPlatform) -> Option<Vec<ShortcutBinding>> {
    let key = parse_key(&alias.key)?;
    let mut modifiers = LogicalModifiers::NONE;
    let mut primary = false;
    for name in &alias.modifiers {
        let modifier = match name.as_str() {
            "Shift" => LogicalModifiers::SHIFT,
            "Control" => LogicalModifiers::CONTROL,
            "Alt" => LogicalModifiers::ALT,
            "Super" => LogicalModifiers::SUPER,
            "Meta" => LogicalModifiers::META,
            "Hyper" => LogicalModifiers::HYPER,
            "Primary" if !primary => {
                primary = true;
                continue;
            }
            _ => return None,
        };
        if modifiers.contains(modifier) {
            return None;
        }
        modifiers = modifiers.union(modifier);
    }
    if primary
        && modifiers.intersects(
            LogicalModifiers::CONTROL
                .union(LogicalModifiers::SUPER)
                .union(LogicalModifiers::META),
        )
    {
        return None;
    }
    let expansions = if primary {
        platform.primary_modifiers().to_vec()
    } else {
        vec![LogicalModifiers::NONE]
    };
    Some(
        expansions
            .into_iter()
            .map(|extra| ShortcutBinding {
                key,
                modifiers: ShortcutModifiers::Exact(modifiers.union(extra)),
            })
            .collect(),
    )
}

fn apply_overrides(
    descriptors: &mut [ShortcutDescriptor],
    platform: ShortcutPlatform,
    overrides: &Overrides,
) {
    let footer_displacements = footer_default_displacements(overrides);
    for descriptor in descriptors {
        let (defaults, aliases) = match platform {
            ShortcutPlatform::MacOs => (
                &mut descriptor.macos_defaults,
                &mut descriptor.macos_aliases,
            ),
            ShortcutPlatform::Portable => (
                &mut descriptor.portable_defaults,
                &mut descriptor.portable_aliases,
            ),
        };
        if descriptor.action == Action::ToggleFooter {
            for claim in defaults.iter_mut().chain(aliases.iter_mut()) {
                claim
                    .contexts
                    .retain(|context| !footer_displacements.contains(context));
            }
            defaults.retain(|claim| !claim.contexts.is_empty());
            aliases.retain(|claim| !claim.contexts.is_empty());
        }
        for ((context, action), bindings) in overrides {
            if *action != descriptor.action {
                continue;
            }
            let contexts = if *action == Action::ToggleFooter && *context == Context::Board {
                &[Context::Board, Context::InsertionBoundary][..]
            } else {
                std::slice::from_ref(context)
            };
            for claim in defaults.iter_mut().chain(aliases.iter_mut()) {
                claim
                    .contexts
                    .retain(|candidate| !contexts.contains(candidate));
            }
            defaults.retain(|claim| !claim.contexts.is_empty());
            aliases.retain(|claim| !claim.contexts.is_empty());
            defaults.extend(bindings.iter().map(|binding| ShortcutBindingClaim {
                binding: *binding,
                contexts: contexts.to_vec(),
                presentation: ShortcutBindingPresentation::Explicit,
            }));
        }
    }
}

fn footer_default_displacements(overrides: &Overrides) -> BTreeSet<Context> {
    overrides
        .iter()
        .filter_map(|((context, action), bindings)| {
            (*action != Action::ToggleFooter
                && bindings.iter().any(|binding| {
                    binding.key == LogicalKey::Character('h')
                        && binding.modifiers == ShortcutModifiers::Exact(LogicalModifiers::NONE)
                }))
            .then_some(*context)
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
