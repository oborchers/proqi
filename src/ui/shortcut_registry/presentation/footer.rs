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
            registry.action_label(context, action, compact_key)
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
