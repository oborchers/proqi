//! Closed execution ownership for every Commands-visible action.

use super::model::ShortcutActionId as Action;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandExecution {
    Board(BoardCommand),
    Editor(EditorCommand),
    Entry(EntryCommand),
    Paste(PasteCommand),
    Runtime(RuntimeCommand),
    Selection(SelectionCommand),
    Submission(SubmissionCommand),
    Transformation(TransformationCommand),
    ReflowThought,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoardCommand {
    New,
    InsertAbove,
    InsertBelow,
    RenameSession,
    CopySessionId,
    CopyResume,
    SendSession,
    SendSessionRemove,
    Delete,
    Copy,
    Cut,
    Duplicate,
    Undo,
    Redo,
    MoveUp,
    MoveDown,
    FocusFirst,
    FocusLast,
    Collapse,
    Help,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditorCommand {
    PlainNewline,
    DeleteLogicalLine,
    DeleteSentence,
    JumpUp,
    JumpDown,
    SelectVisualRowStart,
    SelectVisualRowEnd,
    ThoughtStart,
    ThoughtEnd,
    Indent,
    Outdent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EntryCommand {
    Edit,
    InsertInvocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PasteCommand {
    Exact,
    Reflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeCommand {
    RefreshAgents,
    RefreshAttachments,
    RefreshInvocations,
    CheckUpdates,
    WhatsNew,
    ScreenshotInbox,
    RetryScreenshotCapture,
    RetryStorage,
    ExportRecovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionCommand {
    SelectAll,
    Select,
    RangeSelect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubmissionCommand {
    Remove,
    Keep,
    ToAgent,
    AllRemove,
    AllKeep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransformationCommand {
    SplitThought,
    ExtractSelection,
    MergeThoughts,
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive action-to-executor mapping prevents hidden Commands fallthrough"
)]
pub(crate) const fn execution_for(action: Action) -> Option<CommandExecution> {
    use Action as A;
    use CommandExecution as E;
    match action {
        A::New => Some(E::Board(BoardCommand::New)),
        A::InsertAbove => Some(E::Board(BoardCommand::InsertAbove)),
        A::InsertBelow => Some(E::Board(BoardCommand::InsertBelow)),
        A::RenameSession => Some(E::Board(BoardCommand::RenameSession)),
        A::CopySessionId => Some(E::Board(BoardCommand::CopySessionId)),
        A::CopyResume => Some(E::Board(BoardCommand::CopyResume)),
        A::SendSession => Some(E::Board(BoardCommand::SendSession)),
        A::SendSessionRemove => Some(E::Board(BoardCommand::SendSessionRemove)),
        A::Edit => Some(E::Entry(EntryCommand::Edit)),
        A::PlainNewline => Some(E::Editor(EditorCommand::PlainNewline)),
        A::JumpUp => Some(E::Editor(EditorCommand::JumpUp)),
        A::JumpDown => Some(E::Editor(EditorCommand::JumpDown)),
        A::SelectVisualRowStart => Some(E::Editor(EditorCommand::SelectVisualRowStart)),
        A::SelectVisualRowEnd => Some(E::Editor(EditorCommand::SelectVisualRowEnd)),
        A::ThoughtStart => Some(E::Editor(EditorCommand::ThoughtStart)),
        A::ThoughtEnd => Some(E::Editor(EditorCommand::ThoughtEnd)),
        A::Indent => Some(E::Editor(EditorCommand::Indent)),
        A::Outdent => Some(E::Editor(EditorCommand::Outdent)),
        A::SplitThought => Some(E::Transformation(TransformationCommand::SplitThought)),
        A::ExtractSelection => Some(E::Transformation(TransformationCommand::ExtractSelection)),
        A::MergeThoughts => Some(E::Transformation(TransformationCommand::MergeThoughts)),
        A::Delete => Some(E::Board(BoardCommand::Delete)),
        A::SubmitRemove => Some(E::Submission(SubmissionCommand::Remove)),
        A::SubmitToAgent => Some(E::Submission(SubmissionCommand::ToAgent)),
        A::SubmitAllRemove => Some(E::Submission(SubmissionCommand::AllRemove)),
        A::SubmitAllKeep => Some(E::Submission(SubmissionCommand::AllKeep)),
        A::RefreshAgents => Some(E::Runtime(RuntimeCommand::RefreshAgents)),
        A::RefreshAttachments => Some(E::Runtime(RuntimeCommand::RefreshAttachments)),
        A::InsertInvocation => Some(E::Entry(EntryCommand::InsertInvocation)),
        A::RefreshInvocations => Some(E::Runtime(RuntimeCommand::RefreshInvocations)),
        A::CheckUpdates => Some(E::Runtime(RuntimeCommand::CheckUpdates)),
        A::WhatsNew => Some(E::Runtime(RuntimeCommand::WhatsNew)),
        A::ScreenshotInbox => Some(E::Runtime(RuntimeCommand::ScreenshotInbox)),
        A::RetryScreenshotCapture => Some(E::Runtime(RuntimeCommand::RetryScreenshotCapture)),
        A::RetryStorage => Some(E::Runtime(RuntimeCommand::RetryStorage)),
        A::ExportRecovery => Some(E::Runtime(RuntimeCommand::ExportRecovery)),
        A::MoveUp => Some(E::Board(BoardCommand::MoveUp)),
        A::MoveDown => Some(E::Board(BoardCommand::MoveDown)),
        A::FocusFirst => Some(E::Board(BoardCommand::FocusFirst)),
        A::FocusLast => Some(E::Board(BoardCommand::FocusLast)),
        A::Collapse => Some(E::Board(BoardCommand::Collapse)),
        A::Select => Some(E::Selection(SelectionCommand::Select)),
        A::RangeSelect => Some(E::Selection(SelectionCommand::RangeSelect)),
        A::Help => Some(E::Board(BoardCommand::Help)),
        A::Quit => Some(E::Board(BoardCommand::Quit)),
        A::Copy => Some(E::Board(BoardCommand::Copy)),
        A::Cut => Some(E::Board(BoardCommand::Cut)),
        A::PasteExact => Some(E::Paste(PasteCommand::Exact)),
        A::ReflowThought => Some(E::ReflowThought),
        A::PasteReflow => Some(E::Paste(PasteCommand::Reflow)),
        A::SelectAll => Some(E::Selection(SelectionCommand::SelectAll)),
        A::Duplicate => Some(E::Board(BoardCommand::Duplicate)),
        A::Undo => Some(E::Board(BoardCommand::Undo)),
        A::Redo => Some(E::Board(BoardCommand::Redo)),
        A::SubmitKeep => Some(E::Submission(SubmissionCommand::Keep)),
        A::DeleteLogicalLine => Some(E::Editor(EditorCommand::DeleteLogicalLine)),
        A::DeleteSentence => Some(E::Editor(EditorCommand::DeleteSentence)),
        A::Close
        | A::Confirm
        | A::Backspace
        | A::DeleteForward
        | A::Tab
        | A::BackTab
        | A::FocusPrevious
        | A::FocusNext
        | A::ExtendPrevious
        | A::ExtendNext
        | A::FastPrevious
        | A::FastExtendPrevious
        | A::FastExtendNext
        | A::ExtendFirst
        | A::ExtendLast
        | A::FastNext
        | A::MoveGraphemeBack
        | A::MoveGraphemeForward
        | A::MoveWordBack
        | A::MoveWordForward
        | A::MoveDocumentStart
        | A::MoveDocumentEnd
        | A::MoveVisualUp
        | A::MoveVisualDown
        | A::MoveLineStart
        | A::MoveLineEnd
        | A::ExtendGraphemeBack
        | A::ExtendGraphemeForward
        | A::ExtendWordBack
        | A::ExtendWordForward
        | A::ExtendVisualUp
        | A::ExtendVisualDown
        | A::ExtendDocumentStart
        | A::ExtendDocumentEnd
        | A::ExtendLineStart
        | A::ExtendLineEnd
        | A::ExtendVisualRowStart
        | A::ExtendVisualRowEnd
        | A::MoveVisualRowStart
        | A::MoveVisualRowEnd
        | A::PickerPrevious
        | A::PickerNext
        | A::ContextualTransform
        | A::OpenSearch
        | A::OpenCommands
        | A::BrowserTrash
        | A::ChooseLeft
        | A::ChooseDown
        | A::ChooseUp
        | A::ChooseRight => None,
    }
}
