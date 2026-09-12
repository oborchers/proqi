//! Commands-specific descriptor metadata.

use super::super::Action;
use crate::ui::shortcut_registry::model::{
    CommandApplicability, CommandCategory, CommandDiscoverability, CommandLabel, CommandMetadata,
    CommandRelevance, CommandScope,
};

pub(in crate::ui::shortcut_registry) fn command_metadata(
    action: Action,
    order: usize,
    label: &'static str,
) -> CommandMetadata {
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
        discoverability: CommandDiscoverability::Discoverable,
        applicability: command_applicability(action),
        relevance: command_relevance(action),
        category: command_category(action),
        scope: command_scope(action),
    }
}

const fn command_applicability(action: Action) -> CommandApplicability {
    match action {
        Action::New | Action::RenameSession | Action::PasteExact | Action::PasteReflow => {
            CommandApplicability::WritableBoard
        }
        Action::SubmitRemove | Action::SubmitKeep => CommandApplicability::Submission,
        Action::SubmitAllRemove | Action::SubmitAllKeep => CommandApplicability::SubmissionAll,
        Action::InsertAbove | Action::InsertBelow | Action::Select | Action::RangeSelect => {
            CommandApplicability::BoardThought
        }
        Action::FocusFirst | Action::FocusLast => CommandApplicability::BoardNonempty,
        Action::SelectAll => CommandApplicability::HasThoughts,
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
        | Action::Outdent => CommandApplicability::Editor,
        Action::RetryScreenshotCapture => CommandApplicability::ScreenshotRetry,
        Action::SplitThought => CommandApplicability::Split,
        Action::ExtractSelection => CommandApplicability::Extract,
        Action::MergeThoughts => CommandApplicability::Merge,
        Action::ScreenshotInbox => CommandApplicability::ScreenshotInbox,
        Action::RetryStorage => CommandApplicability::RetryStorage,
        Action::ExportRecovery => CommandApplicability::ExportRecovery,
        Action::Quit => CommandApplicability::Quit,
        Action::Undo => CommandApplicability::Undo,
        Action::Redo => CommandApplicability::Redo,
        Action::RefreshAttachments => CommandApplicability::Attachments,
        Action::WhatsNew => CommandApplicability::InstalledHighlights,
        Action::Copy => CommandApplicability::Copy,
        Action::Cut => CommandApplicability::Cut,
        Action::Delete
        | Action::ReflowThought
        | Action::Duplicate
        | Action::Collapse
        | Action::Edit
        | Action::InsertInvocation
        | Action::SendSession
        | Action::SendSessionRemove
        | Action::SubmitToAgent => CommandApplicability::MutableThought,
        Action::MoveUp | Action::MoveDown => CommandApplicability::Reorder,
        _ => CommandApplicability::Always,
    }
}

const fn command_relevance(action: Action) -> CommandRelevance {
    match action {
        Action::RetryStorage | Action::ExportRecovery => CommandRelevance::StorageRecovery(0),
        Action::RetryScreenshotCapture => CommandRelevance::ScreenshotRetry(1),
        Action::ScreenshotInbox => CommandRelevance::ScreenshotActive(2),
        Action::Undo => CommandRelevance::Undo(3),
        Action::Redo => CommandRelevance::Redo(4),
        Action::MergeThoughts => CommandRelevance::Selection(5),
        Action::ExtractSelection | Action::SplitThought => CommandRelevance::Editor(6),
        Action::SubmitKeep => CommandRelevance::Submission(7),
        Action::New => CommandRelevance::Always(0),
        Action::Edit => CommandRelevance::FocusedThought(20),
        Action::Copy => CommandRelevance::FocusedThought(30),
        Action::PasteExact => CommandRelevance::Always(40),
        Action::RenameSession => CommandRelevance::Always(50),
        Action::CopyResume => CommandRelevance::Always(60),
        Action::Help => CommandRelevance::Always(80),
        Action::Quit => CommandRelevance::Always(90),
        _ => CommandRelevance::Never,
    }
}

const fn command_category(action: Action) -> CommandCategory {
    match action {
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
        | Action::Outdent
        | Action::SplitThought
        | Action::ExtractSelection
        | Action::PasteExact
        | Action::PasteReflow
        | Action::ReflowThought
        | Action::InsertInvocation
        | Action::RefreshInvocations => CommandCategory::Edit,
        Action::MergeThoughts | Action::SelectAll | Action::Select | Action::RangeSelect => {
            CommandCategory::Selection
        }
        Action::SubmitRemove
        | Action::SubmitKeep
        | Action::SubmitToAgent
        | Action::SubmitAllRemove
        | Action::SubmitAllKeep
        | Action::SendSession
        | Action::SendSessionRemove
        | Action::RefreshAgents => CommandCategory::Delivery,
        Action::RenameSession | Action::CopySessionId | Action::CopyResume => {
            CommandCategory::Session
        }
        Action::RefreshAttachments | Action::ScreenshotInbox | Action::RetryScreenshotCapture => {
            CommandCategory::AttachmentsAndCapture
        }
        Action::CheckUpdates
        | Action::WhatsNew
        | Action::RetryStorage
        | Action::ExportRecovery
        | Action::Undo
        | Action::Redo
        | Action::Help
        | Action::Quit => CommandCategory::ApplicationAndRecovery,
        _ => CommandCategory::Thought,
    }
}

const fn command_scope(action: Action) -> CommandScope {
    match action {
        Action::Copy
        | Action::Cut
        | Action::PasteExact
        | Action::PasteReflow
        | Action::SelectAll
        | Action::SubmitRemove
        | Action::SubmitKeep
        | Action::SubmitToAgent
        | Action::Undo
        | Action::Redo => CommandScope::Contextual,
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
        | Action::Outdent
        | Action::SplitThought
        | Action::ExtractSelection
        | Action::InsertInvocation
        | Action::RefreshInvocations => CommandScope::Editor,
        Action::MergeThoughts | Action::Select | Action::RangeSelect => CommandScope::Selection,
        Action::RenameSession
        | Action::CopySessionId
        | Action::CopyResume
        | Action::SendSession
        | Action::SendSessionRemove => CommandScope::Session,
        Action::RefreshAgents
        | Action::RefreshAttachments
        | Action::CheckUpdates
        | Action::WhatsNew
        | Action::ScreenshotInbox
        | Action::RetryScreenshotCapture
        | Action::RetryStorage
        | Action::ExportRecovery
        | Action::Help
        | Action::Quit => CommandScope::Application,
        _ => CommandScope::Board,
    }
}
