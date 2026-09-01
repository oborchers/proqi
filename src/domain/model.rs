//! Principal domain records and aggregate invariants.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    ContentAnnotation, RevisionId, SessionId, TextPosition, ThoughtId, validate_annotations,
};

/// UTC milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Construct a timestamp from UTC milliseconds.
    #[must_use]
    pub const fn from_millis(value: i64) -> Self {
        Self(value)
    }

    /// Return UTC milliseconds since the Unix epoch.
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }
}

/// Monotonic operation number within one session.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationSequence(u64);

impl OperationSequence {
    /// Initial operation sequence.
    pub const ZERO: Self = Self(0);

    /// Construct a sequence.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the primitive value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Return the next sequence when it can be represented.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Zero-based position among live thoughts in a session.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThoughtPosition(u32);

impl ThoughtPosition {
    /// Construct a thought position.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the primitive value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Cardinal direction to an adjacent terminal pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Pane above Proqi.
    Up,
    /// Pane to the right of Proqi.
    Right,
    /// Pane below Proqi.
    Down,
    /// Pane to the left of Proqi.
    Left,
}

impl Direction {
    /// Stable lowercase representation used at external and durable boundaries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Right => "right",
            Self::Down => "down",
            Self::Left => "left",
        }
    }
}

/// A scratchpad session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Stable identity.
    pub id: SessionId,
    /// Optional user-assigned name.
    pub name: Option<String>,
    /// Directory from which the session was created.
    pub origin_cwd: PathBuf,
    /// Directory from which it was most recently opened.
    pub last_opened_cwd: PathBuf,
    /// Creation time.
    pub created_at: Timestamp,
    /// Most recent successful opening time.
    pub last_opened_at: Timestamp,
    /// Most recent content activity time.
    pub last_active_at: Timestamp,
    /// Last operation acknowledged as durable.
    pub last_durable_sequence: OperationSequence,
    /// Soft-deletion time, if in recoverable trash.
    pub deleted_at: Option<Timestamp>,
}

impl Session {
    /// Create a live, unnamed session.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::RelativeDirectory`] when `cwd` is not absolute.
    pub fn new(id: SessionId, cwd: PathBuf, now: Timestamp) -> Result<Self, DomainError> {
        validate_absolute_path(&cwd)?;
        Ok(Self {
            id,
            name: None,
            origin_cwd: cwd.clone(),
            last_opened_cwd: cwd,
            created_at: now,
            last_opened_at: now,
            last_active_at: now,
            last_durable_sequence: OperationSequence::ZERO,
            deleted_at: None,
        })
    }

    /// Rename the session, or clear its optional name.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::BlankSessionName`] for whitespace-only names.
    pub fn rename(&mut self, name: Option<String>) -> Result<(), DomainError> {
        if name.as_deref().is_some_and(|value| value.trim().is_empty()) {
            return Err(DomainError::BlankSessionName);
        }
        self.name = name;
        Ok(())
    }

    /// Validate restored session paths and optional naming invariants.
    ///
    /// # Errors
    ///
    /// Returns a domain error when persisted state bypassed constructor invariants.
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_absolute_path(&self.origin_cwd)?;
        validate_absolute_path(&self.last_opened_cwd)?;
        if self
            .name
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DomainError::BlankSessionName);
        }
        Ok(())
    }

    /// Record a successful open after a lease has been acquired.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::RelativeDirectory`] when `cwd` is not absolute.
    pub fn record_open(&mut self, cwd: PathBuf, now: Timestamp) -> Result<(), DomainError> {
        validate_absolute_path(&cwd)?;
        self.last_opened_cwd = cwd;
        self.last_opened_at = now;
        self.last_active_at = self.last_active_at.max(now);
        Ok(())
    }
}

/// Durable responsive rendering preference for one thought.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThoughtPresentation {
    /// Apply the responsive viewport cap only when content requires it.
    #[default]
    Automatic,
    /// Always render the complete thought and let the board scroll around it.
    Expanded,
    /// Render a compact preview until the user expands it.
    Collapsed,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CompatibleThoughtPresentation {
    Presentation(ThoughtPresentation),
    LegacyCollapsed(bool),
}

fn deserialize_thought_presentation<'de, D>(
    deserializer: D,
) -> Result<ThoughtPresentation, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(
        match CompatibleThoughtPresentation::deserialize(deserializer)? {
            CompatibleThoughtPresentation::Presentation(presentation) => presentation,
            CompatibleThoughtPresentation::LegacyCollapsed(true) => ThoughtPresentation::Collapsed,
            CompatibleThoughtPresentation::LegacyCollapsed(false) => ThoughtPresentation::Automatic,
        },
    )
}

impl ThoughtPresentation {
    /// Stable SQLite and JSON-compatible representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Expanded => "expanded",
            Self::Collapsed => "collapsed",
        }
    }

    /// Parse a durable presentation value.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidThoughtPresentation`] for unknown values.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "automatic" => Ok(Self::Automatic),
            "expanded" => Ok(Self::Expanded),
            "collapsed" => Ok(Self::Collapsed),
            _ => Err(DomainError::InvalidThoughtPresentation(value.to_owned())),
        }
    }

    /// Compatibility projection for the original collapsed boolean contract.
    #[must_use]
    pub const fn is_collapsed(self) -> bool {
        matches!(self, Self::Collapsed)
    }
}

/// One independently editable body of plain text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Thought {
    /// Stable identity.
    pub id: ThoughtId,
    /// Owning session.
    pub session_id: SessionId,
    /// Exact current content.
    pub content: String,
    /// Durable presentation metadata over exact UTF-8 byte ranges.
    #[serde(default)]
    pub annotations: Vec<ContentAnnotation>,
    /// Current order among live thoughts.
    pub position: ThoughtPosition,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last content or structural change.
    pub updated_at: Timestamp,
    /// Durable user presentation preference.
    #[serde(
        default,
        alias = "collapsed",
        deserialize_with = "deserialize_thought_presentation"
    )]
    pub presentation: ThoughtPresentation,
    /// Soft-deletion time, if absent from the live board.
    pub deleted_at: Option<Timestamp>,
}

impl Thought {
    /// Create one live thought.
    #[must_use]
    pub fn new(
        id: ThoughtId,
        session_id: SessionId,
        content: String,
        position: ThoughtPosition,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            session_id,
            content,
            annotations: Vec::new(),
            position,
            created_at: now,
            updated_at: now,
            presentation: ThoughtPresentation::Automatic,
            deleted_at: None,
        }
    }

    /// Whether the thought is visible on its board.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.deleted_at.is_none()
    }

    /// Replace presentation annotations after validating their content ranges.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidContentAnnotation`] for invalid, overlapping,
    /// or non-character-boundary ranges.
    pub fn set_annotations(
        &mut self,
        annotations: Vec<ContentAnnotation>,
    ) -> Result<(), DomainError> {
        validate_annotations(&self.content, &annotations)?;
        self.annotations = annotations;
        Ok(())
    }
}

/// One coalesced editor revision with reversible cursor state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ThoughtRevision {
    /// Stable identity.
    pub id: RevisionId,
    /// Owning session.
    pub session_id: SessionId,
    /// Edited thought.
    pub thought_id: ThoughtId,
    /// Monotonic commit sequence in the owning session.
    pub sequence: OperationSequence,
    /// Content before the edit.
    pub before_content: String,
    /// Content after the edit.
    pub after_content: String,
    /// Presentation metadata before the edit.
    #[serde(default)]
    pub before_annotations: Vec<ContentAnnotation>,
    /// Presentation metadata after the edit.
    #[serde(default)]
    pub after_annotations: Vec<ContentAnnotation>,
    /// Cursor before the edit.
    pub before_cursor: TextPosition,
    /// Cursor after the edit.
    pub after_cursor: TextPosition,
    /// Revision time.
    pub created_at: Timestamp,
}

impl ThoughtRevision {
    /// Validate both durable annotation snapshots against their exact content.
    ///
    /// # Errors
    ///
    /// Returns an annotation error when either history side is malformed.
    pub fn validate_annotations(&self) -> Result<(), DomainError> {
        validate_annotations(&self.before_content, &self.before_annotations)?;
        validate_annotations(&self.after_content, &self.after_annotations)
    }
}

/// Last verified integration context used for recognition only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IntegrationContext {
    /// Integration provider, such as `herdr`.
    pub provider: String,
    /// Last verified adjacent direction.
    pub direction: Direction,
    /// Recognized agent kind.
    pub agent_kind: String,
    /// Human-readable agent name.
    pub agent_name: String,
    /// Non-authoritative workspace hint.
    pub workspace_hint: Option<String>,
    /// Non-authoritative tab hint.
    pub tab_hint: Option<String>,
    /// Non-authoritative pane hint.
    pub pane_hint: Option<String>,
    /// Time at which the context was independently verified.
    pub verified_at: Timestamp,
}

/// Domain validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    /// A durable thought presentation value was unknown.
    #[error("invalid thought presentation: {0}")]
    InvalidThoughtPresentation(String),
    /// Session names cannot be whitespace-only.
    #[error("session name cannot be blank")]
    BlankSessionName,
    /// Session paths must be absolute before entering the domain.
    #[error("session directory must be absolute: {0}")]
    RelativeDirectory(PathBuf),
    /// A thought was applied to another session.
    #[error("thought {thought_id} does not belong to session {session_id}")]
    WrongSession {
        /// Thought with the invalid ownership.
        thought_id: ThoughtId,
        /// Expected session.
        session_id: SessionId,
    },
    /// A referenced thought is not present.
    #[error("thought not found: {0}")]
    ThoughtNotFound(ThoughtId),
    /// A live thought with that identity is already present.
    #[error("thought already exists: {0}")]
    ThoughtAlreadyExists(ThoughtId),
    /// The aggregate contains two retained records with one identity.
    #[error("duplicate retained thought identity: {0}")]
    DuplicateThoughtId(ThoughtId),
    /// A requested position is outside the live board.
    #[error("thought position {requested} exceeds board length {len}")]
    InvalidPosition {
        /// Requested zero-based position.
        requested: usize,
        /// Current live board length.
        len: usize,
    },
    /// Live thought positions are not unique and contiguous.
    #[error("live thought positions are not normalized")]
    NonNormalizedPositions,
    /// The operation sequence cannot increase further.
    #[error("operation sequence exhausted")]
    SequenceExhausted,
    /// Content presentation metadata does not address valid canonical text.
    #[error("content annotation range is invalid")]
    InvalidContentAnnotation,
    /// A requested content range is reversed, outside content, or splits UTF-8.
    #[error("content range is invalid")]
    InvalidContentRange,
    /// An operation requires a non-empty exact content range.
    #[error("content range cannot be empty")]
    EmptyContentRange,
    /// Exact concatenated content cannot be represented on this platform.
    #[error("content length overflow")]
    ContentLengthOverflow,
    /// A reversible replacement no longer matches current thought content.
    #[error("thought content changed before transformation: {0}")]
    ThoughtContentConflict(ThoughtId),
}

fn validate_absolute_path(path: &Path) -> Result<(), DomainError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(DomainError::RelativeDirectory(path.to_path_buf()))
    }
}
