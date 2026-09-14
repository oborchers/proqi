//! Closed active-owner inventory and contextual history policy.

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
    /// Current-server agent target query.
    GlobalDeliveryQuery,
    /// Current-server submission disposition chooser.
    GlobalDeliveryDisposition,
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

impl ShortcutContext {
    pub(crate) fn surface(
        mode: crate::application::InteractionMode,
        insertion: bool,
        failed: bool,
    ) -> Self {
        if failed {
            Self::Recovery
        } else if insertion && matches!(mode, crate::application::InteractionMode::Board) {
            Self::InsertionBoundary
        } else {
            mode.into()
        }
    }

    /// Return the single undo and redo ownership policy for this UI context.
    pub(crate) const fn undo_contract(self) -> crate::application::UndoContract {
        use crate::application::UndoContract as Contract;

        match self {
            Self::Board | Self::InsertionBoundary => Contract::DurableBoard,
            Self::Compose => Contract::ComposeHandoff,
            Self::Edit | Self::Invocation => Contract::DurableEditor,
            Self::Commands
            | Self::Search
            | Self::InvocationQuery
            | Self::Transfer
            | Self::GlobalDeliveryQuery
            | Self::BrowserQuery
            | Self::Rename
            | Self::BrowserRename => Contract::LocalText,
            Self::Browser => Contract::BrowserTextThenDurable,
            Self::Help
            | Self::GlobalDeliveryDisposition
            | Self::Update
            | Self::Screenshot
            | Self::Recovery
            | Self::Direction
            | Self::ReleaseHighlights => Contract::Unavailable,
        }
    }
}

impl From<crate::application::InteractionMode> for ShortcutContext {
    fn from(mode: crate::application::InteractionMode) -> Self {
        match mode {
            crate::application::InteractionMode::Board => Self::Board,
            crate::application::InteractionMode::Compose => Self::Compose,
            crate::application::InteractionMode::Edit { .. } => Self::Edit,
        }
    }
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
