//! Footer copy, labels, and measurement from the resolved action descriptors.

use crate::ui::{ShortcutActionId as Action, ShortcutContext as Context, ShortcutRegistry};

pub(crate) struct FooterProjection {
    pub(crate) key: String,
    pub(crate) text: &'static str,
    pub(crate) minimum_width: u16,
}

pub(crate) fn footer_projection(
    action: Action,
    compact: bool,
    context: Context,
    registry: &ShortcutRegistry,
) -> Option<FooterProjection> {
    let metadata = registry.descriptor(action)?.footer?;
    let compact_key = compact
        || matches!(action, Action::New | Action::Quit)
        || matches!(context, Context::Board | Context::InsertionBoundary)
            && matches!(action, Action::SubmitRemove | Action::SubmitKeep);
    Some(FooterProjection {
        key: if matches!(
            context,
            Context::Compose | Context::Edit | Context::Invocation
        ) && matches!(action, Action::SubmitRemove | Action::SubmitKeep)
        {
            registry.compact_help_label(context, &[action])
        } else {
            super::compact_action_label(registry, context, action, compact_key)
        },
        text: if compact {
            metadata.compact_text
        } else {
            metadata.text
        },
        minimum_width: if compact {
            metadata.compact_minimum_width
        } else {
            metadata.minimum_width
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{KeyBindings, ShortcutPlatform};

    #[test]
    fn macos_history_footer_prefers_terminal_safe_control_labels() {
        let registry = ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::MacOs)
            .expect("factory keymap is valid");
        assert_eq!(
            footer_projection(Action::Undo, false, Context::Board, &registry)
                .expect("undo has footer metadata")
                .key,
            "Ctrl+Z"
        );
        assert_eq!(
            footer_projection(Action::Redo, false, Context::Edit, &registry)
                .expect("redo has footer metadata")
                .key,
            "Ctrl+Shift+Z"
        );
    }

    #[test]
    fn portable_history_footer_keeps_primary_and_board_aliases() {
        let registry =
            ShortcutRegistry::resolve(&KeyBindings::default(), ShortcutPlatform::Portable)
                .expect("factory keymap is valid");
        assert_eq!(
            footer_projection(Action::Undo, false, Context::Board, &registry)
                .expect("undo has footer metadata")
                .key,
            "Ctrl+Z/u"
        );
    }
}
