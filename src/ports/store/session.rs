//! Persisted session snapshots and browser projections.

use serde::{Deserialize, Serialize};

use crate::domain::{
    BoardOperation, IntegrationContext, SessionBoard, SessionId, ThoughtId, ThoughtRevision,
    Timestamp,
};

/// Complete persisted session state and reversible history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    /// Current validated board.
    pub board: SessionBoard,
    /// Structural history in application order.
    pub board_operations: Vec<BoardOperation>,
    /// Applied prefix length of structural history.
    pub board_history_cursor: usize,
    /// Editor revisions retained for every thought.
    pub revisions: Vec<ThoughtRevision>,
    /// Applied editor prefix length for every thought.
    pub editor_history_cursors: Vec<(ThoughtId, usize)>,
    /// Last verified recognition-only integration context.
    pub integration_context: Option<IntegrationContext>,
}

/// Search options for the session browser and CLI.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionQuery {
    /// Optional words matched across names, paths, and current thought content.
    pub text: Option<String>,
    /// Whether recoverably trashed sessions are included.
    pub include_trashed: bool,
    /// Optional current directory used only for ranking.
    pub current_directory: Option<std::path::PathBuf>,
}

/// Lightweight session search result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionHit {
    /// Stable session identity.
    pub id: SessionId,
    /// Optional session name.
    pub name: Option<String>,
    /// Directory in which the session was first created.
    pub origin_cwd: std::path::PathBuf,
    /// Directory from which it was last opened.
    pub last_opened_cwd: std::path::PathBuf,
    /// Latest successful opening time.
    pub last_opened_at: Timestamp,
    /// Latest activity time.
    pub last_active_at: Timestamp,
    /// Number of live thoughts.
    pub thought_count: usize,
    /// Derived first useful content excerpt.
    pub excerpt: String,
    /// First two useful exact-content previews, each independently bounded.
    pub previews: Vec<String>,
    /// Complete live thought corpus used only by the in-memory browser filter.
    #[serde(skip)]
    pub search_content: String,
    /// Last verified adjacent-agent recognition context.
    pub integration_context: Option<IntegrationContext>,
    /// Whether the session is in recoverable trash.
    pub trashed: bool,
}
