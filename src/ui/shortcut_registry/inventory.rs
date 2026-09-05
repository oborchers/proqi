//! Complete descriptor inventory for direct keys and Commands actions.

pub(super) mod bindings;
pub(super) mod metadata;

use std::collections::{BTreeMap, BTreeSet};

use crate::ui::settings::KeyBindings;

use super::model::{
    ShortcutActionId as Action, ShortcutContext as Context, ShortcutDescriptor, ShortcutIntention,
    ShortcutSafety,
};
use bindings::{alias_claims, default_claims};

pub(super) const DIRECT_ACTIONS: &[Action] = &[
    Action::Quit,
    Action::Close,
    Action::Confirm,
    Action::Backspace,
    Action::DeleteForward,
    Action::Tab,
    Action::BackTab,
    Action::FocusPrevious,
    Action::FocusNext,
    Action::ExtendPrevious,
    Action::ExtendNext,
    Action::MoveUp,
    Action::MoveDown,
    Action::FastPrevious,
    Action::FastNext,
    Action::MoveGraphemeBack,
    Action::MoveGraphemeForward,
    Action::MoveWordBack,
    Action::MoveWordForward,
    Action::MoveDocumentStart,
    Action::MoveDocumentEnd,
    Action::MoveVisualUp,
    Action::MoveVisualDown,
    Action::MoveLineStart,
    Action::MoveLineEnd,
    Action::ExtendGraphemeBack,
    Action::ExtendGraphemeForward,
    Action::ExtendWordBack,
    Action::ExtendWordForward,
    Action::ExtendVisualUp,
    Action::ExtendVisualDown,
    Action::ExtendDocumentStart,
    Action::ExtendDocumentEnd,
    Action::ExtendLineStart,
    Action::ExtendLineEnd,
    Action::ExtendVisualRowStart,
    Action::ExtendVisualRowEnd,
    Action::MoveVisualRowStart,
    Action::MoveVisualRowEnd,
    Action::Copy,
    Action::Cut,
    Action::PasteExact,
    Action::PasteReflow,
    Action::SelectAll,
    Action::Duplicate,
    Action::Undo,
    Action::Redo,
    Action::SubmitRemove,
    Action::SubmitKeep,
    Action::DeleteLogicalLine,
    Action::DeleteSentence,
    Action::PickerPrevious,
    Action::PickerNext,
    Action::New,
    Action::Edit,
    Action::Delete,
    Action::Collapse,
    Action::Select,
    Action::ContextualTransform,
    Action::RangeSelect,
    Action::OpenSearch,
    Action::OpenCommands,
    Action::Help,
    Action::ScreenshotInbox,
    Action::RenameSession,
    Action::BrowserTrash,
    Action::RetryStorage,
    Action::ExportRecovery,
    Action::ChooseLeft,
    Action::ChooseDown,
    Action::ChooseUp,
    Action::ChooseRight,
];

pub(super) const ESCAPE_CONTEXTS: &[Context] = &[
    Context::Board,
    Context::InsertionBoundary,
    Context::Compose,
    Context::Edit,
    Context::Help,
    Context::Commands,
    Context::Search,
    Context::Invocation,
    Context::InvocationQuery,
    Context::Transfer,
    Context::Browser,
    Context::BrowserQuery,
    Context::Rename,
    Context::BrowserRename,
    Context::Update,
    Context::Screenshot,
    Context::Direction,
    Context::ReleaseHighlights,
];

pub(super) fn descriptors(keys: &KeyBindings) -> Vec<ShortcutDescriptor> {
    let commands = Action::COMMANDS
        .into_iter()
        .enumerate()
        .map(|(order, (action, label))| (action, metadata::command_metadata(action, order, label)))
        .collect::<BTreeMap<_, _>>();
    let mut actions = DIRECT_ACTIONS.iter().copied().collect::<BTreeSet<_>>();
    actions.extend(commands.keys().copied());
    let macos_defaults = default_claims(true);
    let portable_defaults = default_claims(false);
    let macos_aliases = alias_claims(keys, true);
    let portable_aliases = alias_claims(keys, false);
    actions
        .into_iter()
        .map(|action| {
            descriptor(
                action,
                commands.get(&action).copied(),
                macos_defaults.get(&action).cloned().unwrap_or_default(),
                portable_defaults.get(&action).cloned().unwrap_or_default(),
                macos_aliases.get(&action).cloned().unwrap_or_default(),
                portable_aliases.get(&action).cloned().unwrap_or_default(),
            )
        })
        .collect()
}

fn descriptor(
    action: Action,
    command: Option<super::model::CommandMetadata>,
    macos_defaults: Vec<super::model::ShortcutBindingClaim>,
    portable_defaults: Vec<super::model::ShortcutBindingClaim>,
    macos_aliases: Vec<super::model::ShortcutBindingClaim>,
    portable_aliases: Vec<super::model::ShortcutBindingClaim>,
) -> ShortcutDescriptor {
    let help = metadata::help_metadata(action);
    let mut contexts = macos_defaults
        .iter()
        .chain(&portable_defaults)
        .chain(&macos_aliases)
        .chain(&portable_aliases)
        .flat_map(|claim| claim.contexts.iter().copied())
        .collect::<BTreeSet<_>>();
    contexts.extend(metadata::help_contexts(&help));
    if command.is_some() {
        contexts.insert(Context::Commands);
    }
    let mut contexts = contexts.into_iter().collect::<Vec<_>>();
    contexts.sort_unstable();
    ShortcutDescriptor {
        action,
        contexts,
        macos_defaults,
        portable_defaults,
        macos_aliases,
        portable_aliases,
        safety: safety(action),
        help,
        footer: metadata::footer_metadata(action),
        commands: command,
        diagnostics: action.diagnostics_id(),
        intention: intention(action, command),
    }
}

const fn safety(action: Action) -> ShortcutSafety {
    match action {
        Action::Close => ShortcutSafety::InvariantClose,
        Action::RetryStorage | Action::ExportRecovery | Action::Quit => {
            ShortcutSafety::RecoveryCritical
        }
        Action::Delete | Action::Cut => ShortcutSafety::DestructiveUndoable,
        Action::Backspace
        | Action::DeleteForward
        | Action::DeleteLogicalLine
        | Action::DeleteSentence => ShortcutSafety::TextEditing,
        _ => ShortcutSafety::Ordinary,
    }
}

fn intention(action: Action, command: Option<super::model::CommandMetadata>) -> ShortcutIntention {
    if DIRECT_ACTIONS.contains(&action) {
        match action {
            Action::New
            | Action::Edit
            | Action::Delete
            | Action::Collapse
            | Action::Select
            | Action::ContextualTransform
            | Action::RangeSelect
            | Action::OpenSearch
            | Action::OpenCommands
            | Action::Help
            | Action::ScreenshotInbox
            | Action::BrowserTrash
            | Action::RetryStorage
            | Action::ExportRecovery => ShortcutIntention::TypedAction,
            Action::FocusPrevious
            | Action::FocusNext
            | Action::ExtendPrevious
            | Action::ExtendNext
            | Action::MoveUp
            | Action::MoveDown
            | Action::ChooseLeft
            | Action::ChooseDown
            | Action::ChooseUp
            | Action::ChooseRight => ShortcutIntention::Contextual,
            _ => ShortcutIntention::Existing,
        }
    } else if command.is_some() {
        ShortcutIntention::CommandsOnly
    } else {
        ShortcutIntention::Existing
    }
}
