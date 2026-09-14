//! Labels derived exclusively from the active resolved binding graph.

mod browser;
mod footer;
mod help;

pub(crate) use browser::browser_footer_projection;
pub(crate) use footer::footer_projection;
pub(crate) use help::{HelpItem, help_items};

use super::{
    ShortcutActionId as Action, ShortcutBinding, ShortcutContext as Context, ShortcutModifiers,
    ShortcutPlatform, ShortcutRegistry,
};
use crate::ui::{LogicalKey, LogicalModifiers};

fn compact_action_label(
    registry: &ShortcutRegistry,
    context: Context,
    action: Action,
    compact: bool,
) -> String {
    if registry.platform() == ShortcutPlatform::MacOs
        && matches!(action, Action::Undo | Action::Redo)
    {
        registry.compact_help_label(context, &[action])
    } else {
        registry.action_label(context, action, compact)
    }
}

impl ShortcutRegistry {
    pub(super) fn project_bindings(
        &self,
    ) -> std::collections::BTreeMap<(Context, Action), Vec<ShortcutBinding>> {
        self.descriptors
            .iter()
            .flat_map(|descriptor| {
                descriptor.contexts.iter().map(|context| {
                    (
                        (*context, descriptor.action),
                        self.build_presented_bindings(*context, descriptor.action),
                    )
                })
            })
            .collect()
    }

    pub(crate) fn labels(&self, context: Context, action: Action) -> Vec<String> {
        self.projected_bindings
            .get(&(context, action))
            .into_iter()
            .flatten()
            .map(|binding| self.label_binding(*binding))
            .collect()
    }

    fn build_presented_bindings(&self, context: Context, action: Action) -> Vec<ShortcutBinding> {
        let claims = self.action_claims(context, action);
        let preferred = claims
            .iter()
            .filter(|claim| {
                claim.presentation != super::model::ShortcutBindingPresentation::DispatchOnly
            })
            .map(|claim| claim.binding)
            .collect::<std::collections::BTreeSet<_>>();
        let dispatch_only = claims
            .iter()
            .filter(|claim| {
                claim.presentation == super::model::ShortcutBindingPresentation::DispatchOnly
            })
            .map(|claim| claim.binding)
            .collect::<std::collections::BTreeSet<_>>();
        let mut bindings = claims
            .into_iter()
            .map(|claim| claim.binding)
            .collect::<Vec<_>>();
        bindings.sort_by_key(|binding| {
            let ShortcutModifiers::Exact(modifiers) = binding.modifiers else {
                return (true, true, 3);
            };
            let ordinary = !modifiers.intersects(
                LogicalModifiers::SUPER
                    .union(LogicalModifiers::META)
                    .union(LogicalModifiers::CONTROL),
            );
            let key_order = match binding.key {
                LogicalKey::Character(_) => 0,
                LogicalKey::Up | LogicalKey::Down => 1,
                _ => 2,
            };
            (
                ordinary,
                !modifiers.contains(LogicalModifiers::SHIFT),
                key_order,
            )
        });
        let mut labels = std::collections::BTreeSet::new();
        let mut presented = Vec::new();
        for binding in &bindings {
            if !preferred.contains(binding) && redundant(binding, &bindings)
                || dispatch_only.contains(binding) && vertical_character_alias(binding, &bindings)
                || self.meta_alias(binding, &bindings)
                || uppercase_compatibility(binding, &bindings)
            {
                continue;
            }
            let label = self.label_binding(*binding);
            if labels.insert(label) {
                presented.push(*binding);
            }
        }
        presented
    }

    pub(crate) fn action_label(&self, context: Context, action: Action, compact: bool) -> String {
        let labels = self.labels(context, action);
        if compact {
            labels
                .into_iter()
                .min_by_key(|label| crate::ports::text_layout::terminal_cell_width(label))
                .unwrap_or_default()
        } else {
            labels.join("/")
        }
    }

    fn meta_alias(&self, binding: &ShortcutBinding, bindings: &[ShortcutBinding]) -> bool {
        let ShortcutModifiers::Exact(modifiers) = binding.modifiers else {
            return false;
        };
        self.platform() == ShortcutPlatform::MacOs
            && modifiers.contains(LogicalModifiers::META)
            && bindings.iter().any(|other| {
                other.key == binding.key
                    && other.modifiers
                        == ShortcutModifiers::Exact(
                            modifiers
                                .difference(LogicalModifiers::META)
                                .union(LogicalModifiers::SUPER),
                        )
            })
    }

    #[cfg(test)]
    pub(crate) fn help_label(&self, context: Context, actions: &[Action]) -> String {
        self.help_labels(context, actions).join("/")
    }

    pub(crate) fn help_labels(&self, context: Context, actions: &[Action]) -> Vec<String> {
        let bindings = actions
            .iter()
            .filter_map(|action| self.projected_bindings.get(&(context, *action)))
            .flatten()
            .copied();
        self.group_binding_labels(bindings)
    }

    pub(crate) fn compact_help_label(&self, context: Context, actions: &[Action]) -> String {
        let bindings = actions
            .iter()
            .filter_map(|action| self.compact_binding(context, *action));
        self.group_binding_labels(bindings).join("/")
    }

    fn compact_binding(&self, context: Context, action: Action) -> Option<ShortcutBinding> {
        let claims = self.action_claims(context, action);
        let explicit = claims
            .iter()
            .filter(|claim| {
                claim.presentation == super::model::ShortcutBindingPresentation::Explicit
            })
            .map(|claim| claim.binding)
            .collect::<Vec<_>>();
        let candidates = if explicit.is_empty() {
            self.projected_bindings
                .get(&(context, action))
                .cloned()
                .unwrap_or_default()
        } else {
            explicit
        };
        candidates.into_iter().min_by_key(|binding| {
            let label = self.label_binding(*binding);
            (
                crate::ports::text_layout::terminal_cell_width(&label),
                label,
            )
        })
    }

    fn group_binding_labels(
        &self,
        bindings: impl IntoIterator<Item = ShortcutBinding>,
    ) -> Vec<String> {
        let mut groups: Vec<(LogicalModifiers, Vec<String>)> = Vec::new();
        for binding in bindings {
            let ShortcutModifiers::Exact(modifiers) = binding.modifiers else {
                continue;
            };
            let key = key_label(binding.key, modifiers);
            if let Some((_, keys)) = groups
                .iter_mut()
                .find(|(candidate, _)| *candidate == modifiers)
            {
                keys.extend((!keys.contains(&key)).then_some(key));
            } else {
                groups.push((modifiers, vec![key]));
            }
        }
        groups
            .into_iter()
            .map(|(modifiers, keys)| {
                let prefix = self.modifier_label(modifiers);
                let keys = keys.join("/");
                if prefix.is_empty() {
                    keys
                } else {
                    format!("{prefix}+{keys}")
                }
            })
            .collect()
    }

    fn label_binding(&self, binding: ShortcutBinding) -> String {
        let ShortcutModifiers::Exact(modifiers) = binding.modifiers else {
            return String::new();
        };
        let prefix = self.modifier_label(modifiers);
        let key = key_label(binding.key, modifiers);
        if prefix.is_empty() {
            key
        } else {
            format!("{prefix}+{key}")
        }
    }

    fn modifier_label(&self, modifiers: LogicalModifiers) -> String {
        let mut parts = Vec::new();
        for (flag, name) in [
            (LogicalModifiers::CONTROL, "Ctrl"),
            (
                LogicalModifiers::ALT,
                if self.platform() == ShortcutPlatform::MacOs {
                    "Option"
                } else {
                    "Alt"
                },
            ),
            (
                LogicalModifiers::SUPER,
                if self.platform() == ShortcutPlatform::MacOs {
                    "Cmd"
                } else {
                    "Super"
                },
            ),
            (LogicalModifiers::META, "Meta"),
            (LogicalModifiers::HYPER, "Hyper"),
            (LogicalModifiers::SHIFT, "Shift"),
        ] {
            if modifiers.contains(flag) {
                parts.push(name.to_owned());
            }
        }
        parts.join("+")
    }
}

fn key_label(key: LogicalKey, modifiers: LogicalModifiers) -> String {
    match key {
        LogicalKey::Up => "↑".to_owned(),
        LogicalKey::Down => "↓".to_owned(),
        LogicalKey::Left => "←".to_owned(),
        LogicalKey::Right => "→".to_owned(),
        LogicalKey::Escape => "Esc".to_owned(),
        LogicalKey::Delete => "Del".to_owned(),
        LogicalKey::Character(character)
            if !modifiers.is_empty() && character.is_ascii_lowercase() =>
        {
            character.to_uppercase().collect()
        }
        LogicalKey::Character(character)
            if !modifiers.is_empty() && character.is_ascii_uppercase() =>
        {
            format!("U+{:04X}", u32::from(character))
        }
        LogicalKey::Character(character)
            if crate::ports::text_layout::terminal_cell_width(&character.to_string()) == 0 =>
        {
            format!("U+{:04X}", u32::from(character))
        }
        key => super::contract::key_name(key),
    }
}

fn vertical_character_alias(binding: &ShortcutBinding, bindings: &[ShortcutBinding]) -> bool {
    let arrow = match binding.key {
        LogicalKey::Character('k' | 'K') => LogicalKey::Up,
        LogicalKey::Character('j' | 'J') => LogicalKey::Down,
        _ => return false,
    };
    bindings
        .iter()
        .any(|candidate| candidate.key == arrow && candidate.modifiers == binding.modifiers)
}

fn redundant(binding: &ShortcutBinding, bindings: &[ShortcutBinding]) -> bool {
    let ShortcutModifiers::Exact(modifiers) = binding.modifiers else {
        return false;
    };
    bindings.iter().any(|candidate| {
        let ShortcutModifiers::Exact(other) = candidate.modifiers else {
            return false;
        };
        candidate.key == binding.key && other != modifiers && modifiers.contains(other)
    })
}

fn uppercase_compatibility(binding: &ShortcutBinding, bindings: &[ShortcutBinding]) -> bool {
    let LogicalKey::Character(character) = binding.key else {
        return false;
    };
    let ShortcutModifiers::Exact(modifiers) = binding.modifiers else {
        return false;
    };
    character.is_ascii_uppercase()
        && bindings.iter().any(|candidate| {
            candidate.key == LogicalKey::Character(character.to_ascii_lowercase())
                && (candidate.modifiers == binding.modifiers
                    || !modifiers.contains(LogicalModifiers::SHIFT)
                        && candidate.modifiers
                            == ShortcutModifiers::Exact(modifiers.union(LogicalModifiers::SHIFT)))
        })
}
