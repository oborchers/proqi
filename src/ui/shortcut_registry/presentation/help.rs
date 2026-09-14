//! Contextual Help projected from ordered descriptor metadata and resolved aliases.

use crate::ui::shortcut_registry::{HelpAvailability, HelpSurface};
use crate::ui::{BoardApp, ShortcutActionId as Action, ShortcutContext as Context};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HelpItem {
    pub(crate) full_key: String,
    pub(crate) compact_key: String,
    pub(crate) label: &'static str,
}

pub(crate) fn help_items(app: &BoardApp) -> Vec<HelpItem> {
    let context = app.footer_shortcut_context();
    let surface = match context {
        Context::Recovery => HelpSurface::Recovery,
        Context::Compose | Context::Edit => HelpSurface::Editor,
        _ => HelpSurface::Board,
    };
    let registry = app.shortcut_registry();
    registry
        .help(surface)
        .into_iter()
        .filter(|(_, metadata)| match metadata.availability {
            HelpAvailability::Always | HelpAvailability::EffectiveTransform => true,
            HelpAvailability::Submission => app.supports_submission(),
            HelpAvailability::Undo => app.history_available(true),
            HelpAvailability::Redo => app.history_available(false),
        })
        .filter_map(|(action, metadata)| {
            let actions = related_actions(action);
            let labels = registry.help_labels(context, &actions);
            let full_key = labels.join("/");
            let compact_key = registry.compact_help_label(context, &actions);
            (!full_key.is_empty()).then_some(HelpItem {
                full_key,
                compact_key,
                label: metadata.label,
            })
        })
        .collect()
}

fn related_actions(action: Action) -> Vec<Action> {
    match action {
        Action::FocusNext => vec![Action::FocusNext, Action::FocusPrevious],
        Action::ExtendNext => vec![Action::ExtendNext, Action::ExtendPrevious],
        Action::MoveDown => vec![Action::MoveDown, Action::MoveUp],
        Action::FastNext => vec![Action::FastPrevious, Action::FastNext],
        Action::FastExtendNext => {
            vec![Action::FastExtendPrevious, Action::FastExtendNext]
        }
        Action::MoveDocumentStart => vec![Action::MoveDocumentStart, Action::MoveDocumentEnd],
        Action::MoveLineStart => vec![Action::MoveLineStart, Action::MoveLineEnd],
        Action::ExtendVisualRowStart => {
            vec![Action::ExtendVisualRowStart, Action::ExtendVisualRowEnd]
        }
        Action::MoveVisualDown => vec![Action::MoveVisualUp, Action::MoveVisualDown],
        _ => vec![action],
    }
}
