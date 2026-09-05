//! Closed shortcut identities, contexts, and descriptor metadata.

use crate::ui::{LogicalKey, LogicalModifiers};

mod metadata;

pub(crate) use metadata::{
    CommandAvailability, CommandLabel, CommandMetadata, FooterMetadata, HelpAvailability,
    HelpMetadata, HelpSurface,
};

/// Every active keyboard owner in the current terminal product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShortcutContext {
    /// Whole-thought board input.
    Board,
    /// Transient empty-thought editor input.
    Compose,
    /// Durable thought editor input.
    Edit,
    /// Contextual Help overlay.
    Help,
    /// Searchable Commands overlay.
    Commands,
    /// Thought search overlay.
    Search,
    /// Invocation completion or query overlay.
    Invocation,
    /// Explicit invocation search field, distinct from editor-backed completion.
    InvocationQuery,
    /// Cross-session transfer query.
    Transfer,
    /// Empty session-browser query, where management aliases remain active.
    Browser,
    /// Nonempty session-browser query.
    BrowserQuery,
    /// Session-name text field in either application surface.
    Rename,
    /// Session Browser rename field, whose Delete contract differs from Board rename.
    BrowserRename,
    /// Update-choice overlay.
    Update,
    /// Screenshot Inbox takeover choice or quit confirmation.
    Screenshot,
    /// Failed-durability recovery owner.
    Recovery,
    /// Adjacent-agent direction chooser.
    Direction,
    /// Scrollable packaged release highlights.
    ReleaseHighlights,
    /// Armed two-step insertion-boundary confirmation.
    InsertionBoundary,
}

/// Explicit bottom-to-top active keyboard ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortcutContextStack(Vec<ShortcutContext>);

impl ShortcutContextStack {
    /// Construct a stack whose last item is the active owner.
    #[must_use]
    pub fn new(contexts: impl IntoIterator<Item = ShortcutContext>) -> Self {
        Self(contexts.into_iter().collect())
    }

    /// Return the active top owner.
    #[must_use]
    pub fn active(&self) -> Option<ShortcutContext> {
        self.0.last().copied()
    }

    /// Inspect all owners from underlying surface to top overlay.
    #[must_use]
    pub fn as_slice(&self) -> &[ShortcutContext] {
        &self.0
    }
}

/// Stable content-free identity for every current semantic keyboard action.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(missing_docs)]
pub enum ShortcutActionId {
    New,
    RenameSession,
    CopySessionId,
    CopyResume,
    SendSession,
    SendSessionRemove,
    Edit,
    PlainNewline,
    JumpUp,
    JumpDown,
    SelectVisualRowStart,
    SelectVisualRowEnd,
    ThoughtStart,
    ThoughtEnd,
    Indent,
    Outdent,
    SplitThought,
    ExtractSelection,
    MergeThoughts,
    Delete,
    SubmitRemove,
    SubmitAllRemove,
    SubmitAllKeep,
    RefreshAgents,
    RefreshAttachments,
    InsertInvocation,
    RefreshInvocations,
    CheckUpdates,
    WhatsNew,
    ScreenshotInbox,
    RetryScreenshotCapture,
    RetryStorage,
    ExportRecovery,
    MoveUp,
    MoveDown,
    Collapse,
    Select,
    RangeSelect,
    Help,
    Quit,
    Close,
    Confirm,
    Backspace,
    DeleteForward,
    Tab,
    BackTab,
    FocusPrevious,
    FocusNext,
    ExtendPrevious,
    ExtendNext,
    FastPrevious,
    FastNext,
    MoveGraphemeBack,
    MoveGraphemeForward,
    MoveWordBack,
    MoveWordForward,
    MoveDocumentStart,
    MoveDocumentEnd,
    MoveVisualUp,
    MoveVisualDown,
    MoveLineStart,
    MoveLineEnd,
    ExtendGraphemeBack,
    ExtendGraphemeForward,
    ExtendWordBack,
    ExtendWordForward,
    ExtendVisualUp,
    ExtendVisualDown,
    ExtendDocumentStart,
    ExtendDocumentEnd,
    ExtendLineStart,
    ExtendLineEnd,
    ExtendVisualRowStart,
    ExtendVisualRowEnd,
    MoveVisualRowStart,
    MoveVisualRowEnd,
    Copy,
    Cut,
    PasteExact,
    PasteReflow,
    SelectAll,
    Duplicate,
    Undo,
    Redo,
    SubmitKeep,
    DeleteLogicalLine,
    DeleteSentence,
    PickerPrevious,
    PickerNext,
    ContextualTransform,
    OpenSearch,
    OpenCommands,
    BrowserTrash,
    ChooseLeft,
    ChooseDown,
    ChooseUp,
    ChooseRight,
}

impl ShortcutActionId {
    /// Complete visible Commands inventory in its established order.
    pub(crate) const COMMANDS: [(Self, &'static str); 51] = [
        (Self::New, "New thought"),
        (Self::RenameSession, "Rename session"),
        (Self::CopySessionId, "Copy session ID"),
        (Self::CopyResume, "Copy resume command"),
        (Self::Edit, "Edit thought"),
        (Self::PlainNewline, "Insert plain newline"),
        (Self::DeleteLogicalLine, "Delete logical line"),
        (Self::DeleteSentence, "Delete sentence"),
        (
            Self::JumpUp,
            "Jump cursor up 5 visual rows (Alt+↑ or Page Up)",
        ),
        (
            Self::JumpDown,
            "Jump cursor down 5 visual rows (Alt+↓ or Page Down)",
        ),
        (
            Self::SelectVisualRowStart,
            "Extend selection to visual row start",
        ),
        (
            Self::SelectVisualRowEnd,
            "Extend selection to visual row end",
        ),
        (Self::ThoughtStart, "Move cursor to thought beginning"),
        (Self::ThoughtEnd, "Move cursor to thought end"),
        (Self::Indent, "Indent line or selection"),
        (Self::Outdent, "Outdent line or selection"),
        (Self::SplitThought, "Split thought at cursor"),
        (Self::ExtractSelection, "Extract selection as new thought"),
        (Self::MergeThoughts, "Merge selected thoughts"),
        (Self::Delete, "Delete thought"),
        (Self::Copy, "Copy thought"),
        (Self::Cut, "Cut thought"),
        (Self::PasteExact, "Paste exactly"),
        (Self::PasteReflow, "Paste and reflow"),
        (Self::Duplicate, "Duplicate thought or selection"),
        (Self::SelectAll, "Select all thoughts"),
        (Self::SubmitRemove, "Submit"),
        (Self::SubmitKeep, "Submit and keep"),
        (Self::SubmitAllRemove, "Submit all"),
        (Self::SubmitAllKeep, "Submit all and keep"),
        (Self::SendSession, "Send to another Proqi session"),
        (
            Self::SendSessionRemove,
            "Send to another Proqi session and remove thought",
        ),
        (Self::RefreshAgents, "Refresh adjacent agents"),
        (Self::RefreshAttachments, "Refresh attachments"),
        (Self::InsertInvocation, "Insert discovered invocation"),
        (Self::RefreshInvocations, "Refresh invocations"),
        (Self::CheckUpdates, "Check for updates"),
        (Self::WhatsNew, "What's new"),
        (Self::ScreenshotInbox, "Enable Screenshot Inbox"),
        (Self::RetryScreenshotCapture, "Retry Screenshot Capture"),
        (Self::RetryStorage, "Retry failed save"),
        (Self::ExportRecovery, "Export recovery file"),
        (Self::Undo, "Undo board action"),
        (Self::Redo, "Redo board action"),
        (Self::MoveUp, "Move thought up"),
        (Self::MoveDown, "Move thought down"),
        (Self::Collapse, "Expand or collapse thought"),
        (Self::Select, "Toggle thought selection"),
        (Self::RangeSelect, "Start contiguous range selection"),
        (Self::Help, "Open contextual help"),
        (Self::Quit, "Quit Proqi"),
    ];

    /// Stable content-free diagnostics spelling.
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(
            clippy::enum_glob_use,
            reason = "the exhaustive stable diagnostics contract is clearest as one closed mapping"
        )
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive stable diagnostics contract is clearest as one closed mapping"
    )]
    pub const fn diagnostics_id(self) -> &'static str {
        use ShortcutActionId::*;
        match self {
            New => "thought.new",
            RenameSession => "session.rename",
            CopySessionId => "session.copy_id",
            CopyResume => "session.copy_resume",
            SendSession => "session.send",
            SendSessionRemove => "session.send_remove",
            Edit => "thought.edit",
            PlainNewline => "editor.plain_newline",
            JumpUp => "editor.jump_up",
            JumpDown => "editor.jump_down",
            SelectVisualRowStart => "editor.select_visual_row_start",
            SelectVisualRowEnd => "editor.select_visual_row_end",
            ThoughtStart => "editor.thought_start",
            ThoughtEnd => "editor.thought_end",
            Indent => "editor.indent",
            Outdent => "editor.outdent",
            SplitThought => "thought.split",
            ExtractSelection => "thought.extract_selection",
            MergeThoughts => "thought.merge",
            Delete => "thought.delete",
            SubmitRemove => "submission.submit_remove",
            SubmitAllRemove => "submission.submit_all_remove",
            SubmitAllKeep => "submission.submit_all_keep",
            RefreshAgents => "agents.refresh",
            RefreshAttachments => "attachments.refresh",
            InsertInvocation => "invocation.insert",
            RefreshInvocations => "invocation.refresh",
            CheckUpdates => "update.check",
            WhatsNew => "update.whats_new",
            ScreenshotInbox => "screenshot.inbox",
            RetryScreenshotCapture => "screenshot.retry_capture",
            RetryStorage => "recovery.retry_storage",
            ExportRecovery => "recovery.export",
            MoveUp => "thought.move_up",
            MoveDown => "thought.move_down",
            Collapse => "thought.collapse",
            Select => "thought.select",
            RangeSelect => "thought.range_select",
            Help => "help.open",
            Quit => "application.quit",
            Close => "context.close",
            Confirm => "context.confirm",
            Backspace => "text.backspace",
            DeleteForward => "text.delete_forward",
            Tab => "text.tab",
            BackTab => "text.backtab",
            FocusPrevious => "list.previous",
            FocusNext => "list.next",
            ExtendPrevious => "board.range_previous",
            ExtendNext => "board.range_next",
            FastPrevious => "navigation.fast_previous",
            FastNext => "navigation.fast_next",
            MoveGraphemeBack => "editor.grapheme_back",
            MoveGraphemeForward => "editor.grapheme_forward",
            MoveWordBack => "editor.word_back",
            MoveWordForward => "editor.word_forward",
            MoveDocumentStart => "editor.document_start",
            MoveDocumentEnd => "editor.document_end",
            MoveVisualUp => "editor.visual_up",
            MoveVisualDown => "editor.visual_down",
            MoveLineStart => "editor.line_start",
            MoveLineEnd => "editor.line_end",
            ExtendGraphemeBack => "editor.extend_grapheme_back",
            ExtendGraphemeForward => "editor.extend_grapheme_forward",
            ExtendWordBack => "editor.extend_word_back",
            ExtendWordForward => "editor.extend_word_forward",
            ExtendVisualUp => "editor.extend_visual_up",
            ExtendVisualDown => "editor.extend_visual_down",
            ExtendDocumentStart => "editor.extend_document_start",
            ExtendDocumentEnd => "editor.extend_document_end",
            ExtendLineStart => "editor.extend_line_start",
            ExtendLineEnd => "editor.extend_line_end",
            ExtendVisualRowStart => "editor.extend_visual_row_start",
            ExtendVisualRowEnd => "editor.extend_visual_row_end",
            MoveVisualRowStart => "editor.move_visual_row_start",
            MoveVisualRowEnd => "editor.move_visual_row_end",
            Copy => "clipboard.copy",
            Cut => "clipboard.cut",
            PasteExact => "clipboard.paste_exact",
            PasteReflow => "clipboard.paste_reflow",
            SelectAll => "selection.select_all",
            Duplicate => "board.duplicate",
            Undo => "history.undo",
            Redo => "history.redo",
            SubmitKeep => "submission.submit_keep",
            DeleteLogicalLine => "editor.delete_logical_line",
            DeleteSentence => "editor.delete_sentence",
            PickerPrevious => "invocation.previous",
            PickerNext => "invocation.next",
            ContextualTransform => "thought.contextual_transform",
            OpenSearch => "search.open",
            OpenCommands => "commands.open",
            BrowserTrash => "browser.trash",
            ChooseLeft => "direction.left",
            ChooseDown => "direction.down",
            ChooseUp => "direction.up",
            ChooseRight => "direction.right",
        }
    }
}

/// Safety policy attached to a semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum ShortcutSafety {
    Ordinary,
    TextEditing,
    DestructiveUndoable,
    InvariantClose,
    RecoveryCritical,
}

/// Modifier policy retained by a platform-independent default binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShortcutModifiers {
    /// Match one exact set of raw logical modifiers.
    Exact(LogicalModifiers),
    /// Expand to Super or Meta on macOS and Control elsewhere.
    Primary,
    /// Primary plus a distinct Shift modifier.
    PrimaryShift,
    /// Ignore modifiers that the owning context declares irrelevant.
    Contextual,
}

/// Registry-owned spelling of one default or compatible alias.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ShortcutBinding {
    pub(crate) key: LogicalKey,
    pub(crate) modifiers: ShortcutModifiers,
}

/// Whether a default binding is also the canonical source for Primary-key copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutBindingPresentation {
    DispatchOnly,
    Primary { canonical: bool },
}

/// One effective binding together with the contexts in which it claims input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortcutBindingClaim {
    pub(crate) binding: ShortcutBinding,
    pub(crate) contexts: Vec<ShortcutContext>,
    pub(crate) presentation: ShortcutBindingPresentation,
}

/// How a registry action reaches the existing terminal-independent UI contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutIntention {
    /// The dispatcher maps this action to an existing `UiKey` intention.
    Existing,
    /// The dispatcher forwards the stable action identity itself.
    TypedAction,
    /// The exact intention depends on context or modifier ladder.
    Contextual,
    /// The action currently has no direct key and is selected through Commands.
    CommandsOnly,
}

/// Registry metadata consumed by dispatch, diagnostics, and presentation parity tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortcutDescriptor {
    pub(crate) action: ShortcutActionId,
    pub(crate) contexts: Vec<ShortcutContext>,
    pub(crate) macos_defaults: Vec<ShortcutBindingClaim>,
    pub(crate) portable_defaults: Vec<ShortcutBindingClaim>,
    pub(crate) macos_aliases: Vec<ShortcutBindingClaim>,
    pub(crate) portable_aliases: Vec<ShortcutBindingClaim>,
    pub(crate) safety: ShortcutSafety,
    pub(crate) help: Vec<HelpMetadata>,
    pub(crate) footer: Option<FooterMetadata>,
    pub(crate) commands: Option<CommandMetadata>,
    pub(crate) diagnostics: &'static str,
    pub(crate) intention: ShortcutIntention,
}
