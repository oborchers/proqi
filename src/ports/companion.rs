//! Companion-pane host used by a multiplexer plugin to toggle Proqi beside an agent.
//!
//! The port speaks in panes, tabs, and Proqi sessions. It never exposes a
//! multiplexer command, JSON document, or process identifier above the adapter.

use std::path::PathBuf;

use crate::domain::SessionId;

/// Invocation context captured by the multiplexer when the user asked for the toggle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionContext {
    /// Stable workspace identity.
    pub workspace_id: String,
    /// User-facing workspace label, when the host supplied one.
    pub workspace_label: Option<String>,
    /// Stable tab identity that scopes the companion.
    pub tab_id: String,
    /// User-facing tab label, when the host supplied one.
    pub tab_label: Option<String>,
    /// Pane that had focus when the toggle was invoked.
    pub focused_pane_id: String,
    /// Working directory of the focused pane.
    pub focused_pane_cwd: PathBuf,
    /// Stable directory that anchors the tab's first session: the Herdr worktree
    /// checkout, else the repository root containing the focused directory.
    pub session_root: Option<PathBuf>,
}

/// One pane of the invoking tab, observed from one bounded host snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneObservation {
    /// Stable pane identity.
    pub pane_id: String,
    /// Whether the host reports this pane as focused.
    pub focused: bool,
    /// Whether the host recognizes a coding agent in this pane.
    pub agent: bool,
    /// Whether a running Proqi is present in this pane.
    pub presence: ProqiPresence,
    /// Persisted pane label, when one exists.
    pub label: Option<String>,
    /// Pane working directory, when the host reports one.
    pub cwd: Option<PathBuf>,
}

/// Whether a pane holds a running Proqi, as far as the host can tell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProqiPresence {
    /// Proqi publishes its display lease here or is the foreground process.
    Present,
    /// The host reports no Proqi here.
    Absent,
    /// The host could not report the pane in time, so it may hide a Proqi.
    Unknown,
}

/// Foreground process state of one pane, reduced to what the toggle may act on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaneProcess {
    /// Proqi is running and resumed the given session, when its argument is readable.
    Proqi {
        /// Session named by the `--resume` argument.
        session_id: Option<SessionId>,
    },
    /// The plugin launcher is still replacing itself with Proqi.
    Launcher,
    /// Only the pane's interactive shell is running, with no foreground job.
    IdleShell,
    /// Anything else. The toggle never closes such a pane.
    Other,
    /// The host could not classify the pane in time. It is never closed.
    Unknown,
}

/// A tab's Proqi session and, while it is open, the pane this plugin opened for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionRecord {
    /// Tab the companion belongs to.
    pub tab_id: String,
    /// Pane the plugin opened, or `None` after it closed.
    pub pane_id: Option<String>,
    /// Proqi session the tab uses.
    pub session_id: SessionId,
}

/// Typed companion-host failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CompanionError {
    /// The process is not running as a Herdr plugin action.
    #[error("{0}")]
    Unavailable(String),
    /// The host rejected or failed a request.
    #[error("{0}")]
    Host(String),
    /// Plugin state could not be read or written.
    #[error("{0}")]
    State(String),
}

/// Pane operations the toggle needs from a terminal multiplexer.
pub trait CompanionHost {
    /// Return the invocation context.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the context is missing or malformed.
    fn context(&mut self) -> Result<CompanionContext, CompanionError>;

    /// Observe every pane of one tab from one bounded snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the host cannot be queried.
    fn tab_panes(&mut self, tab_id: &str) -> Result<Vec<PaneObservation>, CompanionError>;

    /// Return the host names of the live agents in one tab; unnamed agents are omitted.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the host cannot be queried.
    fn tab_agent_names(&mut self, tab_id: &str) -> Result<Vec<String>, CompanionError>;

    /// Classify the foreground process of one pane, or `None` when it no longer exists.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the host cannot be queried.
    fn process(&mut self, pane_id: &str) -> Result<Option<PaneProcess>, CompanionError>;

    /// Open Proqi for `session_id` in a new pane to the right of `target_pane_id`.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the pane cannot be opened.
    fn open_beside(
        &mut self,
        target_pane_id: &str,
        cwd: &std::path::Path,
        session_id: SessionId,
    ) -> Result<String, CompanionError>;

    /// Focus one pane.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the pane cannot be focused.
    fn focus(&mut self, pane_id: &str) -> Result<(), CompanionError>;

    /// Close one pane and its process.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError`] when the pane cannot be closed.
    fn close(&mut self, pane_id: &str) -> Result<(), CompanionError>;

    /// Show one best-effort, content-free message to the user.
    fn notify(&mut self, message: &str);
}

/// Exclusive per-plugin state holding at most one companion record per tab.
///
/// A record outlives its pane so that every later toggle in the tab reopens the
/// same session, whichever pane is focused.
pub trait CompanionRecords {
    /// Return every retained record in stable tab order.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError::State`] when the state cannot be read.
    fn all(&mut self) -> Result<Vec<CompanionRecord>, CompanionError>;

    /// Return the record for one tab.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError::State`] when the state cannot be read.
    fn load(&mut self, tab_id: &str) -> Result<Option<CompanionRecord>, CompanionError> {
        Ok(self
            .all()?
            .into_iter()
            .find(|record| record.tab_id == tab_id))
    }

    /// Replace the record for its tab.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError::State`] when the state cannot be written.
    fn save(&mut self, record: &CompanionRecord) -> Result<(), CompanionError>;
}

/// Whether an existing Proqi session can be opened in a new pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompanionSessionState {
    /// No process owns the session.
    Resumable,
    /// Another process owns the session lease.
    Active,
    /// The session no longer exists or is in trash.
    Unavailable,
}

/// Proqi session operations the toggle composes, implemented over the session service.
pub trait CompanionSessions {
    /// Failure reported by the session service.
    type Error: std::fmt::Display;

    /// Return the one live session with this exact name and origin, creating it when absent.
    ///
    /// # Errors
    ///
    /// Returns the service error, including name conflicts and ambiguity.
    fn ensure(&mut self, name: &str, cwd: &std::path::Path) -> Result<SessionId, Self::Error>;

    /// Report whether one session can be opened now.
    ///
    /// # Errors
    ///
    /// Returns the service error when runtime state cannot be inspected.
    fn state(&mut self, session_id: SessionId) -> Result<CompanionSessionState, Self::Error>;

    /// Ask an active owner to make pending editor work durable.
    ///
    /// # Errors
    ///
    /// Returns the service error when a live owner cannot confirm the flush.
    fn flush(&mut self, session_id: SessionId) -> Result<(), Self::Error>;
}
