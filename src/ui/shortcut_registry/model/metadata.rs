//! Typed presentation and visibility policy owned by action descriptors.

/// Underlying surface whose contextual Help includes an action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelpSurface {
    Board,
    Editor,
    Recovery,
}

/// Capability that controls whether a Help item is currently visible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelpAvailability {
    Always,
    Submission,
    EffectiveTransform,
}

/// One ordered Help projection attached to its semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelpMetadata {
    pub(crate) surface: HelpSurface,
    pub(crate) order: u8,
    pub(crate) label: &'static str,
    pub(crate) availability: HelpAvailability,
}

/// Footer copy and measurement policy attached to its semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FooterMetadata {
    pub(crate) text: &'static str,
    pub(crate) compact_text: &'static str,
    pub(crate) minimum_width: u16,
    pub(crate) compact_minimum_width: u16,
}

/// Whether one action belongs to the complete Commands inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandDiscoverability {
    Discoverable,
}

/// Runtime predicate that controls whether one discovered command can execute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandApplicability {
    Always,
    WritableBoard,
    HasThoughts,
    BoardNonempty,
    Copy,
    Cut,
    MutableThought,
    Reorder,
    BoardThought,
    Submission,
    SubmissionAll,
    Editor,
    ScreenshotRetry,
    Split,
    Extract,
    Merge,
    ScreenshotInbox,
    RetryStorage,
    ExportRecovery,
    Quit,
    Undo,
    Redo,
    Attachments,
    InstalledHighlights,
}

/// Semantic condition and priority for the concise empty-query projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandRelevance {
    Never,
    Always(u8),
    FocusedThought(u8),
    Selection(u8),
    Editor(u8),
    Submission(u8),
    Undo(u8),
    Redo(u8),
    StorageRecovery(u8),
    ScreenshotActive(u8),
    ScreenshotRetry(u8),
}

/// Quiet grouping used only by the expanded Commands projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum CommandCategory {
    Thought,
    Edit,
    Selection,
    Delivery,
    Session,
    AttachmentsAndCapture,
    ApplicationAndRecovery,
}

impl CommandCategory {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Thought => "Thought",
            Self::Edit => "Edit",
            Self::Selection => "Selection",
            Self::Delivery => "Delivery",
            Self::Session => "Session",
            Self::AttachmentsAndCapture => "Attachments and capture",
            Self::ApplicationAndRecovery => "Application and recovery",
        }
    }
}

/// Context label and binding owner presented beside a command when space permits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandScope {
    Contextual,
    Board,
    Editor,
    Selection,
    Session,
    Application,
}

impl CommandScope {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Contextual => "current",
            Self::Board => "board",
            Self::Editor => "edit",
            Self::Selection => "selection",
            Self::Session => "session",
            Self::Application => "app",
        }
    }
}

/// Stable or state-dependent Commands label policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandLabel {
    Static(&'static str),
    ScreenshotInbox {
        enable: &'static str,
        disable: &'static str,
        resume: &'static str,
        unavailable: &'static str,
    },
}

/// Ordered Commands projection attached to its semantic action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommandMetadata {
    pub(crate) order: u8,
    pub(crate) label: CommandLabel,
    pub(crate) discoverability: CommandDiscoverability,
    pub(crate) applicability: CommandApplicability,
    pub(crate) relevance: CommandRelevance,
    pub(crate) category: CommandCategory,
    pub(crate) scope: CommandScope,
}
