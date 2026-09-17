//! Payload-free structural Board items and their typed identities.

use serde::{Deserialize, Serialize};

use super::{SeparatorId, SessionId, Thought, ThoughtId, ThoughtPosition, Timestamp};

/// Identity of one durable item in the shared Board order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum BoardItemId {
    /// Editable text thought.
    Thought(ThoughtId),
    /// Payload-free visual separator.
    Separator(SeparatorId),
}

impl BoardItemId {
    /// Return the thought identity when this item is editable text.
    #[must_use]
    pub const fn thought(self) -> Option<ThoughtId> {
        match self {
            Self::Thought(id) => Some(id),
            Self::Separator(_) => None,
        }
    }
}

impl From<ThoughtId> for BoardItemId {
    fn from(value: ThoughtId) -> Self {
        Self::Thought(value)
    }
}

impl From<SeparatorId> for BoardItemId {
    fn from(value: SeparatorId) -> Self {
        Self::Separator(value)
    }
}

/// One durable payload-free visual boundary in a session Board.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Separator {
    /// Stable identity.
    pub id: SeparatorId,
    /// Owning session.
    pub session_id: SessionId,
    /// Current order among all live Board items.
    pub position: ThoughtPosition,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last structural change.
    pub updated_at: Timestamp,
    /// Soft-deletion time, if absent from the live Board.
    pub deleted_at: Option<Timestamp>,
}

impl Separator {
    /// Create one live separator.
    #[must_use]
    pub const fn new(
        id: SeparatorId,
        session_id: SessionId,
        position: ThoughtPosition,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            session_id,
            position,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    /// Whether the separator is visible on its Board.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.deleted_at.is_none()
    }
}

/// Borrowed item in the canonical ordered Board projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardItemRef<'a> {
    /// Editable text thought.
    Thought(&'a Thought),
    /// Payload-free visual separator.
    Separator(&'a Separator),
}

impl<'a> BoardItemRef<'a> {
    /// Stable typed identity.
    #[must_use]
    pub const fn id(self) -> BoardItemId {
        match self {
            Self::Thought(thought) => BoardItemId::Thought(thought.id),
            Self::Separator(separator) => BoardItemId::Separator(separator.id),
        }
    }

    /// Shared live Board position.
    #[must_use]
    pub const fn position(self) -> ThoughtPosition {
        match self {
            Self::Thought(thought) => thought.position,
            Self::Separator(separator) => separator.position,
        }
    }

    /// Editable text payload, if this item is a thought.
    #[must_use]
    pub const fn thought(self) -> Option<&'a Thought> {
        match self {
            Self::Thought(thought) => Some(thought),
            Self::Separator(_) => None,
        }
    }
}
