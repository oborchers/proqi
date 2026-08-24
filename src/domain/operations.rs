//! Reversible board operations and the session board aggregate.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    DomainError, OperationId, OperationSequence, Session, SessionId, Thought, ThoughtId,
    ThoughtPosition, Timestamp, validate_annotations,
};

/// Explicit persistent undo scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "scope")]
pub enum UndoScope {
    /// Structural session operations.
    Board,
    /// Editor revisions for one thought.
    Editor {
        /// Thought whose editor history is addressed.
        thought_id: ThoughtId,
    },
}

/// Kind of structural operation shown in history and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardOperationKind {
    /// Created a thought, including paste-to-create.
    Create,
    /// Deleted a thought without touching the clipboard.
    Delete,
    /// Deleted a thought after a successful clipboard write.
    Cut,
    /// Reordered one thought.
    Reorder,
    /// Changed the explicit collapse preference.
    Collapse,
    /// Deleted after an accepted adjacent-agent submission.
    SubmitAndRemove,
}

/// One reversible change to current board state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mutation")]
pub enum BoardMutation {
    /// Add a new thought or restore the same previously undone creation.
    AddThought {
        /// Complete thought snapshot.
        thought: Thought,
    },
    /// Change recoverable deletion state and restore position when needed.
    SetDeletion {
        /// Affected thought.
        thought_id: ThoughtId,
        /// Deletion time, or `None` to restore.
        deleted_at: Option<Timestamp>,
        /// Position occupied before deletion or desired after restoration.
        position: ThoughtPosition,
    },
    /// Move a live thought between normalized positions.
    MoveThought {
        /// Affected thought.
        thought_id: ThoughtId,
        /// Required current position.
        from: ThoughtPosition,
        /// Desired position.
        to: ThoughtPosition,
    },
    /// Set the explicit collapse preference.
    SetCollapsed {
        /// Affected thought.
        thought_id: ThoughtId,
        /// New preference.
        collapsed: bool,
    },
}

/// Durable operation with a complete inverse payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BoardOperation {
    /// Stable identity.
    pub id: OperationId,
    /// Owning session.
    pub session_id: SessionId,
    /// Monotonic sequence.
    pub sequence: OperationSequence,
    /// Semantic operation kind.
    pub kind: BoardOperationKind,
    /// Mutation used for first apply and redo.
    pub forward: BoardMutation,
    /// Mutation used for undo.
    pub inverse: BoardMutation,
    /// Operation creation time.
    pub created_at: Timestamp,
}

/// Validated session plus all live and recoverably deleted thoughts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionBoard {
    /// Session metadata.
    pub session: Session,
    thoughts: Vec<Thought>,
}

impl SessionBoard {
    /// Validate and construct a session board.
    ///
    /// # Errors
    ///
    /// Returns a domain error when ownership, identity, or live positions are invalid.
    pub fn new(session: Session, thoughts: Vec<Thought>) -> Result<Self, DomainError> {
        let board = Self { session, thoughts };
        board.validate()?;
        Ok(board)
    }

    /// All thoughts, including recoverably deleted records.
    #[must_use]
    pub fn thoughts(&self) -> &[Thought] {
        &self.thoughts
    }

    /// Live thoughts ordered by normalized position.
    #[must_use]
    pub fn live_thoughts(&self) -> Vec<&Thought> {
        let mut thoughts: Vec<_> = self
            .thoughts
            .iter()
            .filter(|thought| thought.is_live())
            .collect();
        thoughts.sort_by_key(|thought| thought.position);
        thoughts
    }

    /// Look up any retained thought.
    #[must_use]
    pub fn thought(&self, id: ThoughtId) -> Option<&Thought> {
        self.thoughts.iter().find(|thought| thought.id == id)
    }

    /// Look up any retained thought mutably.
    pub fn thought_mut(&mut self, id: ThoughtId) -> Option<&mut Thought> {
        self.thoughts.iter_mut().find(|thought| thought.id == id)
    }

    /// Apply one already validated operation mutation.
    ///
    /// # Errors
    ///
    /// Returns a domain error when the mutation cannot preserve board invariants.
    pub fn apply_mutation(
        &mut self,
        mutation: &BoardMutation,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        let mut candidate = self.clone();
        candidate.apply_mutation_in_place(mutation, at)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn apply_mutation_in_place(
        &mut self,
        mutation: &BoardMutation,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        match mutation {
            BoardMutation::AddThought { thought } => self.add_or_restore(thought.clone(), at)?,
            BoardMutation::SetDeletion {
                thought_id,
                deleted_at,
                position,
            } => self.set_deletion(*thought_id, *deleted_at, *position, at)?,
            BoardMutation::MoveThought {
                thought_id,
                from,
                to,
            } => self.move_thought(*thought_id, *from, *to, at)?,
            BoardMutation::SetCollapsed {
                thought_id,
                collapsed,
            } => {
                let thought = self
                    .thought_mut(*thought_id)
                    .ok_or(DomainError::ThoughtNotFound(*thought_id))?;
                thought.collapsed = *collapsed;
                thought.updated_at = at;
            }
        }
        self.session.last_active_at = self.session.last_active_at.max(at);
        Ok(())
    }

    /// Validate ownership and normalized live ordering.
    ///
    /// # Errors
    ///
    /// Returns a domain error for duplicate identities, wrong ownership, or invalid positions.
    pub fn validate(&self) -> Result<(), DomainError> {
        let mut identities = HashSet::with_capacity(self.thoughts.len());
        for thought in &self.thoughts {
            if !identities.insert(thought.id) {
                return Err(DomainError::DuplicateThoughtId(thought.id));
            }
            if thought.session_id != self.session.id {
                return Err(DomainError::WrongSession {
                    thought_id: thought.id,
                    session_id: self.session.id,
                });
            }
            validate_annotations(&thought.content, &thought.annotations)?;
        }
        for (expected, thought) in self.live_thoughts().into_iter().enumerate() {
            if usize::try_from(thought.position.get()).ok() != Some(expected) {
                return Err(DomainError::NonNormalizedPositions);
            }
        }
        Ok(())
    }

    fn add_or_restore(&mut self, mut thought: Thought, at: Timestamp) -> Result<(), DomainError> {
        if thought.session_id != self.session.id {
            return Err(DomainError::WrongSession {
                thought_id: thought.id,
                session_id: self.session.id,
            });
        }
        let target = usize::try_from(thought.position.get()).unwrap_or(usize::MAX);
        let live_len = self.live_thoughts().len();
        if target > live_len {
            return Err(DomainError::InvalidPosition {
                requested: target,
                len: live_len,
            });
        }
        if let Some(existing) = self.thought(thought.id) {
            if existing.is_live() {
                return Err(DomainError::ThoughtAlreadyExists(thought.id));
            }
            self.shift_for_insert(target);
            let existing = self
                .thought_mut(thought.id)
                .ok_or(DomainError::ThoughtNotFound(thought.id))?;
            existing.deleted_at = None;
            existing.position = ThoughtPosition::new(to_u32(target)?);
            existing.updated_at = at;
            return Ok(());
        }
        self.shift_for_insert(target);
        thought.position = ThoughtPosition::new(to_u32(target)?);
        thought.deleted_at = None;
        thought.updated_at = at;
        self.thoughts.push(thought);
        Ok(())
    }

    fn set_deletion(
        &mut self,
        thought_id: ThoughtId,
        deleted_at: Option<Timestamp>,
        position: ThoughtPosition,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        let current = self
            .thought(thought_id)
            .ok_or(DomainError::ThoughtNotFound(thought_id))?
            .clone();
        match (current.deleted_at, deleted_at) {
            (None, Some(deleted)) => {
                let removed = usize::try_from(current.position.get()).unwrap_or(usize::MAX);
                let thought = self
                    .thought_mut(thought_id)
                    .ok_or(DomainError::ThoughtNotFound(thought_id))?;
                thought.deleted_at = Some(deleted);
                thought.updated_at = at;
                self.shift_after_remove(removed);
            }
            (Some(_), None) => {
                let target = usize::try_from(position.get()).unwrap_or(usize::MAX);
                let len = self.live_thoughts().len();
                if target > len {
                    return Err(DomainError::InvalidPosition {
                        requested: target,
                        len,
                    });
                }
                self.shift_for_insert(target);
                let thought = self
                    .thought_mut(thought_id)
                    .ok_or(DomainError::ThoughtNotFound(thought_id))?;
                thought.deleted_at = None;
                thought.position = position;
                thought.updated_at = at;
            }
            (None, None) | (Some(_), Some(_)) => {}
        }
        Ok(())
    }

    fn move_thought(
        &mut self,
        thought_id: ThoughtId,
        from: ThoughtPosition,
        to: ThoughtPosition,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        let len = self.live_thoughts().len();
        let from = usize::try_from(from.get()).unwrap_or(usize::MAX);
        let to = usize::try_from(to.get()).unwrap_or(usize::MAX);
        if from >= len || to >= len {
            return Err(DomainError::InvalidPosition {
                requested: from.max(to),
                len,
            });
        }
        let current = self
            .thought(thought_id)
            .ok_or(DomainError::ThoughtNotFound(thought_id))?;
        if !current.is_live() || usize::try_from(current.position.get()).ok() != Some(from) {
            return Err(DomainError::InvalidPosition {
                requested: from,
                len,
            });
        }
        if from < to {
            shift_range(&mut self.thoughts, from, to, ShiftDirection::TowardStart)?;
        } else if to < from {
            shift_range(&mut self.thoughts, to, from, ShiftDirection::TowardEnd)?;
        }
        let thought = self
            .thought_mut(thought_id)
            .ok_or(DomainError::ThoughtNotFound(thought_id))?;
        thought.position = ThoughtPosition::new(to_u32(to)?);
        thought.updated_at = at;
        Ok(())
    }

    fn shift_for_insert(&mut self, target: usize) {
        for thought in self.thoughts.iter_mut().filter(|thought| thought.is_live()) {
            if usize::try_from(thought.position.get()).unwrap_or(usize::MAX) >= target {
                thought.position = ThoughtPosition::new(thought.position.get().saturating_add(1));
            }
        }
    }

    fn shift_after_remove(&mut self, removed: usize) {
        for thought in self.thoughts.iter_mut().filter(|thought| thought.is_live()) {
            if usize::try_from(thought.position.get()).unwrap_or(usize::MAX) > removed {
                thought.position = ThoughtPosition::new(thought.position.get() - 1);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ShiftDirection {
    TowardStart,
    TowardEnd,
}

fn shift_range(
    thoughts: &mut [Thought],
    start: usize,
    end: usize,
    direction: ShiftDirection,
) -> Result<(), DomainError> {
    for thought in thoughts.iter_mut().filter(|thought| thought.is_live()) {
        let position = usize::try_from(thought.position.get()).unwrap_or(usize::MAX);
        let shifted = match direction {
            ShiftDirection::TowardStart if position > start && position <= end => position - 1,
            ShiftDirection::TowardEnd if position >= start && position < end => position + 1,
            _ => continue,
        };
        thought.position = ThoughtPosition::new(to_u32(shifted)?);
    }
    Ok(())
}

fn to_u32(value: usize) -> Result<u32, DomainError> {
    u32::try_from(value).map_err(|_| DomainError::InvalidPosition {
        requested: value,
        len: usize::try_from(u32::MAX).unwrap_or(usize::MAX),
    })
}
