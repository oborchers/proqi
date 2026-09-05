//! Canonical presentation metadata attached to registry descriptors.

use super::{Action, Context};
use crate::ui::shortcut_registry::model::{
    CommandAvailability, CommandLabel, CommandMetadata, FooterMetadata, HelpAvailability,
    HelpMetadata, HelpSurface,
};

const fn help(
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
        Action::ExtendNext,
        help(HelpSurface::Board, 3, "Range", HelpAvailability::Always),
    ),
    (
        Action::MoveDown,
        help(HelpSurface::Board, 4, "Reorder", HelpAvailability::Always),
    ),
    (
        Action::Copy,
        help(HelpSurface::Board, 5, "Copy", HelpAvailability::Always),
    ),
    (
        Action::Cut,
        help(HelpSurface::Board, 6, "Cut", HelpAvailability::Always),
    ),
    (
        Action::Delete,
        help(HelpSurface::Board, 7, "Delete", HelpAvailability::Always),
    ),
    (
        Action::Duplicate,
        help(HelpSurface::Board, 8, "Duplicate", HelpAvailability::Always),
    ),
    (
        Action::Select,
        help(HelpSurface::Board, 9, "Select", HelpAvailability::Always),
    ),
    (
        Action::ContextualTransform,
        help(
            HelpSurface::Board,
            10,
            "Transform",
            HelpAvailability::EffectiveTransform,
        ),
    ),
    (
        Action::SelectAll,
        help(
            HelpSurface::Board,
            11,
            "Select all",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::RangeSelect,
        help(HelpSurface::Board, 12, "Latch", HelpAvailability::Always),
    ),
    (
        Action::Undo,
        help(HelpSurface::Board, 13, "Undo", HelpAvailability::Always),
    ),
    (
        Action::PasteExact,
        help(
            HelpSurface::Board,
            14,
            "Paste exactly",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::PasteReflow,
        help(
            HelpSurface::Board,
            15,
            "Paste reflow",
            HelpAvailability::Always,
        ),
    ),
    (
        Action::Redo,
        help(HelpSurface::Board, 16, "Redo", HelpAvailability::Always),
    ),
    (
        Action::Collapse,
        help(HelpSurface::Board, 17, "Collapse", HelpAvailability::Always),
    ),
    (
        Action::OpenSearch,
        help(HelpSurface::Board, 18, "Search", HelpAvailability::Always),
    ),
    (
        Action::OpenCommands,
        help(HelpSurface::Board, 19, "Commands", HelpAvailability::Always),
    ),
    (
        Action::ScreenshotInbox,
        help(HelpSurface::Board, 20, "Inbox", HelpAvailability::Always),
    ),
    (
        Action::SubmitRemove,
        help(
            HelpSurface::Board,
            21,
            "Submit",
            HelpAvailability::Submission,
        ),
    ),
    (
        Action::SubmitKeep,
        help(
            HelpSurface::Board,
            22,
            "Submit & keep",
            HelpAvailability::Submission,
        ),
    ),
    (
        Action::Quit,
        help(HelpSurface::Board, 23, "Quit", HelpAvailability::Always),
    ),
    (
        Action::Close,
        help(HelpSurface::Board, 24, "Close", HelpAvailability::Always),
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
            "Paste reflow",
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
            "5-row · PgUp/PgDn",
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
    HELP.iter()
        .filter_map(|(candidate, metadata)| (*candidate == action).then_some(*metadata))
        .collect()
}

pub(super) fn help_contexts(metadata: &[HelpMetadata]) -> impl Iterator<Item = Context> + '_ {
    metadata
        .iter()
        .flat_map(|item| match item.surface {
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

pub(in crate::ui::shortcut_registry) fn command_metadata(
    action: Action,
    order: usize,
    label: &'static str,
) -> CommandMetadata {
    let availability = match action {
        Action::SubmitRemove
        | Action::SubmitKeep
        | Action::SubmitAllRemove
        | Action::SubmitAllKeep => CommandAvailability::Submission,
        Action::PlainNewline
        | Action::DeleteLogicalLine
        | Action::DeleteSentence
        | Action::JumpUp
        | Action::JumpDown
        | Action::SelectVisualRowStart
        | Action::SelectVisualRowEnd
        | Action::ThoughtStart
        | Action::ThoughtEnd
        | Action::Indent
        | Action::Outdent => CommandAvailability::Editor,
        Action::RetryScreenshotCapture => CommandAvailability::ScreenshotRetry,
        Action::SplitThought => CommandAvailability::Split,
        Action::ExtractSelection => CommandAvailability::Extract,
        Action::MergeThoughts => CommandAvailability::Merge,
        Action::ScreenshotInbox => CommandAvailability::ScreenshotInbox,
        _ => CommandAvailability::Always,
    };
    let label = if action == Action::ScreenshotInbox {
        CommandLabel::ScreenshotInbox {
            enable: label,
            disable: "Disable Screenshot Inbox",
            resume: "Resume Screenshot Inbox",
            unavailable: "Screenshot Inbox unavailable",
        }
    } else {
        CommandLabel::Static(label)
    };
    CommandMetadata {
        order: u8::try_from(order).unwrap_or(u8::MAX),
        label,
        availability,
    }
}
