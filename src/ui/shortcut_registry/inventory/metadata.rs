//! Canonical presentation metadata attached to registry descriptors.

mod boundary_help;
mod commands;

use super::{Action, Context};
use crate::ui::shortcut_registry::model::{
    FooterMetadata, HelpAvailability, HelpMetadata, HelpSurface,
};

pub(in crate::ui::shortcut_registry) use commands::command_metadata;

pub(super) const fn help(
    surface: HelpSurface,
    order: u8,
    label: &'static str,
    availability: HelpAvailability,
) -> HelpMetadata {
    HelpMetadata {
        surface,
        order,
        label,
        availability,
    }
}

const HELP: &[(Action, HelpMetadata)] = &[
    (
        Action::ReflowThought,
        help(
            HelpSurface::Board,
            27,
            "Clean up spacing",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::ReflowThought,
        help(
            HelpSurface::Editor,
            17,
            "Clean up spacing",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::RetryStorage,
        help(
            HelpSurface::Recovery,
            0,
            "Retry failed save",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::ExportRecovery,
        help(
            HelpSurface::Recovery,
            1,
            "Export recovery",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::Quit,
        help(
            HelpSurface::Recovery,
            2,
            "Quit after recovery",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::New,
        help(HelpSurface::Board, 0, "New", HelpAvailability::Always),
    ),
    (
        Action::Edit,
        help(HelpSurface::Board, 1, "Edit", HelpAvailability::Always),
    ),
    (
        Action::FocusNext,
        help(
            HelpSurface::Board,
            2,
            "Move/new×2",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::FastNext,
        help(HelpSurface::Board, 3, "Move 5", HelpAvailability::Always),
    ),
    (
        Action::ExtendNext,
        help(HelpSurface::Board, 4, "Range", HelpAvailability::Always),
    ),
    (
        Action::FastExtendNext,
        help(HelpSurface::Board, 5, "Range 5", HelpAvailability::Always),
    ),
    (
        Action::MoveDown,
        help(HelpSurface::Board, 6, "Reorder", HelpAvailability::Always),
    ),
    (
        Action::Copy,
        help(HelpSurface::Board, 7, "Copy", HelpAvailability::Always),
    ),
    (
        Action::Cut,
        help(HelpSurface::Board, 8, "Cut", HelpAvailability::Always),
    ),
    (
        Action::Delete,
        help(HelpSurface::Board, 9, "Delete", HelpAvailability::Always),
    ),
    (
        Action::Duplicate,
        help(
            HelpSurface::Board,
            10,
            "Duplicate",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::Select,
        help(HelpSurface::Board, 11, "Select", HelpAvailability::Always),
    ),
    (
        Action::ContextualTransform,
        help(
            HelpSurface::Board,
            12,
            "Transform",
            HelpAvailability::EffectiveTransform,
        ),
    ),
    (
        Action::SelectAll,
        help(
            HelpSurface::Board,
            13,
            "Select all",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::RangeSelect,
        help(HelpSurface::Board, 14, "Latch", HelpAvailability::Always),
    ),
    (
        Action::Undo,
        help(HelpSurface::Board, 15, "Undo", HelpAvailability::Always),
    ),
    (
        Action::PasteExact,
        help(
            HelpSurface::Board,
            16,
            "Paste exactly",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::PasteReflow,
        help(
            HelpSurface::Board,
            17,
            "Paste and clean up",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::Redo,
        help(HelpSurface::Board, 18, "Redo", HelpAvailability::Always),
    ),
    (
        Action::Collapse,
        help(HelpSurface::Board, 19, "Collapse", HelpAvailability::Always),
    ),
    (
        Action::OpenSearch,
        help(HelpSurface::Board, 20, "Search", HelpAvailability::Always),
    ),
    (
        Action::OpenCommands,
        help(HelpSurface::Board, 21, "Commands", HelpAvailability::Always),
    ),
    (
        Action::ScreenshotInbox,
        help(HelpSurface::Board, 22, "Inbox", HelpAvailability::Always),
    ),
    (
        Action::SubmitRemove,
        help(
            HelpSurface::Board,
            23,
            "Submit",
            HelpAvailability::Submission,
        ),
    ),
    (
        Action::SubmitKeep,
        help(
            HelpSurface::Board,
            24,
            "Submit & keep",
            HelpAvailability::Submission,
        ),
    ),
    (
        Action::Quit,
        help(HelpSurface::Board, 25, "Quit", HelpAvailability::Always),
    ),
    (
        Action::Close,
        help(HelpSurface::Board, 26, "Close", HelpAvailability::Always),
    ),
    (
        Action::Close,
        help(HelpSurface::Editor, 0, "Close", HelpAvailability::Always),
    ),
    (
        Action::SubmitRemove,
        help(
            HelpSurface::Editor,
            1,
            "Submit",
            HelpAvailability::Submission,
        ),
    ),
    (
        Action::SubmitKeep,
        help(
            HelpSurface::Editor,
            2,
            "Submit & keep",
            HelpAvailability::Submission,
        ),
    ),
    (
        Action::Copy,
        help(HelpSurface::Editor, 3, "Copy", HelpAvailability::Always),
    ),
    (
        Action::Cut,
        help(HelpSurface::Editor, 4, "Cut", HelpAvailability::Always),
    ),
    (
        Action::PasteExact,
        help(
            HelpSurface::Editor,
            5,
            "Paste exactly",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::PasteReflow,
        help(
            HelpSurface::Editor,
            6,
            "Paste and clean up",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::SelectAll,
        help(
            HelpSurface::Editor,
            7,
            "Select all",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::DeleteLogicalLine,
        help(
            HelpSurface::Editor,
            8,
            "Delete logical line",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::DeleteSentence,
        help(
            HelpSurface::Editor,
            9,
            "Delete sentence",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::Undo,
        help(HelpSurface::Editor, 10, "Undo", HelpAvailability::Always),
    ),
    (
        Action::Redo,
        help(HelpSurface::Editor, 11, "Redo", HelpAvailability::Always),
    ),
    (
        Action::ContextualTransform,
        help(
            HelpSurface::Editor,
            12,
            "Split/extract",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::FastNext,
        help(
            HelpSurface::Editor,
            13,
            "Move 5 rows",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::MoveDocumentStart,
        help(
            HelpSurface::Editor,
            14,
            "Start/end",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::MoveLineStart,
        help(
            HelpSurface::Editor,
            18,
            "Line start/end",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::ExtendVisualRowStart,
        help(
            HelpSurface::Editor,
            15,
            "Select visual row",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::MoveVisualDown,
        help(
            HelpSurface::Editor,
            16,
            "Neighbor/new",
            HelpAvailability::Always,
        ),
    ),
];

pub(in crate::ui::shortcut_registry) fn help_metadata(action: Action) -> Vec<HelpMetadata> {
    boundary_help::HELP
        .iter()
        .chain(HELP)
        .filter_map(|(candidate, metadata)| (*candidate == action).then_some(*metadata))
        .collect()
}

pub(super) fn help_contexts(metadata: &[HelpMetadata]) -> impl Iterator<Item = Context> + '_ {
    metadata
        .iter()
        .flat_map(|item| match item.surface {
            HelpSurface::Recovery => [Some(Context::Recovery), None],
            HelpSurface::Board => [Some(Context::Board), None],
            HelpSurface::Editor => [Some(Context::Compose), Some(Context::Edit)],
        })
        .flatten()
}

pub(in crate::ui::shortcut_registry) const fn footer_metadata(
    action: Action,
) -> Option<FooterMetadata> {
    let (text, compact_text, minimum_width, compact_minimum_width) = match action {
        Action::New => (" New", " New", 7, 7),
        Action::Copy => (" Copy", " Copy", 7, 7),
        Action::Cut => (" Cut", " Cut", 6, 6),
        Action::Delete => ("", "", 6, 6),
        Action::Select => (" Select", " Select", 12, 12),
        Action::Undo => (" Undo", " Undo", 7, 7),
        Action::OpenSearch => (" Search", " Search", 9, 9),
        Action::OpenCommands => (" Commands", " Menu", 11, 6),
        Action::Help => (" Shortcuts", " Help", 12, 6),
        Action::Quit => (" Quit", " Quit", 0, 0),
        Action::Close => (" Board", "", 10, 3),
        Action::RetryStorage => (" Retry", " Retry", 8, 8),
        Action::ExportRecovery => (" Export", " Export", 10, 10),
        Action::SubmitRemove => (" Submit", " Submit", 9, 9),
        Action::SubmitKeep => (" Submit & keep", " Submit & keep", 16, 16),
        _ => return None,
    };
    Some(FooterMetadata {
        text,
        compact_text,
        minimum_width,
        compact_minimum_width,
    })
}
