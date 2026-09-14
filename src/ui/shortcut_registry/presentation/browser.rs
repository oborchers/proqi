//! Session Browser footer projected from effective action bindings.

use crate::ui::ShortcutActionId as Action;

use super::super::{ShortcutContext, ShortcutRegistry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowserFooterProjection {
    pub(crate) actions: &'static [Action],
    pub(crate) key: String,
    pub(crate) label: String,
}

const RENAME: &[Action] = &[Action::RenameSession];
const TRASH: &[Action] = &[Action::BrowserTrash];
const SELECT: &[Action] = &[Action::FocusPrevious, Action::FocusNext];
const OPEN: &[Action] = &[Action::Confirm];
const CANCEL: &[Action] = &[Action::Close];
const UNDO: &[Action] = &[Action::Undo];
const REDO: &[Action] = &[Action::Redo];

pub(crate) fn browser_footer_projection(
    registry: &ShortcutRegistry,
    width: u16,
    context: ShortcutContext,
    destructive_label: &'static str,
    undo_label: Option<&'static str>,
    redo_label: Option<&'static str>,
) -> Vec<BrowserFooterProjection> {
    let items: &[(&[Action], &str)] = if context == ShortcutContext::BrowserRename {
        &[(OPEN, "Save"), (CANCEL, "Cancel")]
    } else if width >= 60 {
        &[
            (RENAME, "Rename"),
            (TRASH, "Trash"),
            (SELECT, "Select"),
            (OPEN, "Open"),
            (CANCEL, "Cancel"),
        ]
    } else if width >= 36 {
        &[
            (RENAME, "Rename"),
            (TRASH, "Trash"),
            (OPEN, "Open"),
            (CANCEL, "Back"),
        ]
    } else {
        &[(RENAME, "Name"), (TRASH, "Trash"), (CANCEL, "Back")]
    };
    let mut items = items
        .iter()
        .map(|(actions, label)| {
            let key = actions
                .iter()
                .map(|action| registry.action_label(context, *action, true))
                .filter(|label| !label.is_empty())
                .collect::<Vec<_>>()
                .join("/");
            BrowserFooterProjection {
                actions,
                key,
                label: (*label).to_owned(),
            }
        })
        .collect::<Vec<_>>();
    if context == ShortcutContext::Browser
        && let Some(item) = items.iter_mut().find(|item| item.actions == TRASH)
    {
        destructive_label.clone_into(&mut item.label);
    }
    for (actions, label) in [(UNDO, undo_label), (REDO, redo_label)] {
        let Some(target) = label else {
            continue;
        };
        let key = super::compact_action_label(registry, context, actions[0], true);
        if !key.is_empty() {
            items.insert(
                items.len().saturating_sub(1),
                BrowserFooterProjection {
                    actions,
                    key,
                    label: format!("{} {target}", if actions == UNDO { "Undo" } else { "Redo" }),
                },
            );
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{KeyBindings, ShortcutPlatform};

    #[test]
    fn established_responsive_browser_footer_is_registry_projected() {
        let registry = ShortcutRegistry::from_validated(&KeyBindings::default());
        let wide =
            browser_footer_projection(&registry, 80, ShortcutContext::Browser, "Trash", None, None);
        assert_eq!(
            wide.iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>(),
            ["F2", "F8", "↑/↓", "Enter", "Esc"]
        );
        assert_eq!(
            wide.iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["Rename", "Trash", "Select", "Open", "Cancel"]
        );
        assert_eq!(
            browser_footer_projection(
                &registry,
                40,
                ShortcutContext::Browser,
                "Trash",
                None,
                None,
            )[3]
                .label,
            "Back"
        );
        assert_eq!(
            browser_footer_projection(
                &registry,
                30,
                ShortcutContext::Browser,
                "Trash",
                None,
                None,
            )[0]
                .label,
            "Name"
        );
    }

    #[test]
    fn macos_browser_history_prefers_terminal_safe_control_labels() {
        let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
            .expect("factory keymap is valid");
        let items = browser_footer_projection(
            &registry,
            80,
            ShortcutContext::Browser,
            "Trash",
            Some("Rename"),
            Some("Rename"),
        );
        let history = items
            .iter()
            .filter(|item| item.actions == UNDO || item.actions == REDO)
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(history, ["Ctrl+Z", "Ctrl+Shift+Z"]);
    }
}
