//! Reversible board operations and the session board aggregate.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{
    DomainError, OperationId, OperationSequence, Session, SessionId, Thought, ThoughtId,
    ThoughtPosition, ThoughtPresentation, Timestamp, validate_annotations,
};

mod mutation;

/// Durable structural operation record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationRecord {
    /// Stable operation identity.
    pub id: OperationId,
    /// Owning session.
    pub session_id: SessionId,
    /// Monotonic sequence in the session.
    pub sequence: OperationSequence,
    /// Creation time.
    pub created_at: Timestamp,
}

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
    /// Duplicated one or more thoughts as one operation.
    Duplicate,
    /// Deleted after an accepted adjacent-agent submission.
    SubmitAndRemove,
    /// Split one thought at an exact logical cursor.
    Split,
    /// Extract one exact editor selection into a neighboring thought.
    Extract,
    /// Merge a contiguous board selection into its first thought.
    Merge,
}

/// One reversible change to current board state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mutation")]
pub enum BoardMutation {
    /// Apply several structural changes as one reversible history entry.
    Batch {
        /// Ordered mutations whose combined result preserves board invariants.
        mutations: Vec<BoardMutation>,
    },
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
    /// Change deletion state only when the complete transformation source still matches.
    SetDeletionExact {
        /// Affected thought.
        thought_id: ThoughtId,
        /// Required current content.
        expected_content: String,
        /// Required current annotations.
        expected_annotations: Vec<super::ContentAnnotation>,
        /// Required current deletion state.
        expected_deleted_at: Option<Timestamp>,
        /// Required retained position.
        expected_position: ThoughtPosition,
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
    /// Replace exact thought content and all attached annotation ranges.
    ReplaceContent {
        /// Affected thought.
        thought_id: ThoughtId,
        /// Required current content.
        before_content: String,
        /// Required current annotations.
        before_annotations: Vec<super::ContentAnnotation>,
        /// Replacement content.
        after_content: String,
        /// Replacement annotations.
        after_annotations: Vec<super::ContentAnnotation>,
    },
    /// Set the durable presentation preference.
    SetPresentation {
        /// Affected thought.
        thought_id: ThoughtId,
        /// New preference.
        presentation: ThoughtPresentation,
    },
    /// Legacy v0.1.x payload retained only for lossless history migration.
    #[doc(hidden)]
    #[serde(rename = "set_collapsed")]
    LegacySetCollapsed {
        /// Affected thought.
        thought_id: ThoughtId,
        /// Original boolean presentation state.
        collapsed: bool,
    },
}

impl BoardMutation {
    /// Validate every complete thought snapshot carried by this mutation.
    ///
    /// # Errors
    ///
    /// Returns an annotation error for malformed current or dormant history data.
    pub fn validate_annotations(&self) -> Result<(), DomainError> {
        match self {
            Self::Batch { mutations } => {
                for mutation in mutations {
                    mutation.validate_annotations()?;
                }
                Ok(())
            }
            Self::AddThought { thought } => {
                validate_annotations(&thought.content, &thought.annotations)
            }
            Self::ReplaceContent {
                before_content,
                before_annotations,
                after_content,
                after_annotations,
                ..
            } => {
                validate_annotations(before_content, before_annotations)?;
                validate_annotations(after_content, after_annotations)
            }
            Self::SetDeletionExact {
                expected_content,
                expected_annotations,
                ..
            } => validate_annotations(expected_content, expected_annotations),
            Self::SetDeletion { .. }
            | Self::MoveThought { .. }
            | Self::SetPresentation { .. }
            | Self::LegacySetCollapsed { .. } => Ok(()),
        }
    }

    /// Whether this mutation addresses one thought identity.
    #[must_use]
    pub fn addresses(&self, thought_id: ThoughtId) -> bool {
        match self {
            Self::Batch { mutations } => mutations
                .iter()
                .any(|mutation| mutation.addresses(thought_id)),
            Self::AddThought { thought } => thought.id == thought_id,
            Self::SetDeletion {
                thought_id: affected,
                ..
            }
            | Self::SetDeletionExact {
                thought_id: affected,
                ..
            }
            | Self::MoveThought {
                thought_id: affected,
                ..
            }
            | Self::ReplaceContent {
                thought_id: affected,
                ..
            }
            | Self::SetPresentation {
                thought_id: affected,
                ..
            }
            | Self::LegacySetCollapsed {
                thought_id: affected,
                ..
            } => *affected == thought_id,
        }
    }
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

impl BoardOperation {
    /// Validate annotation-bearing forward and inverse history payloads.
    ///
    /// # Errors
    ///
    /// Returns an annotation error when dormant undo or redo state is malformed.
    pub fn validate_annotations(&self) -> Result<(), DomainError> {
        self.forward.validate_annotations()?;
        self.inverse.validate_annotations()
    }
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
            BoardMutation::Batch { mutations } => {
                for mutation in mutations {
                    self.apply_mutation_in_place(mutation, at)?;
                }
            }
            BoardMutation::AddThought { thought } => self.add_or_restore(thought.clone(), at)?,
            BoardMutation::SetDeletion {
                thought_id,
                deleted_at,
                position,
            } => self.set_deletion(*thought_id, *deleted_at, *position, at)?,
            BoardMutation::SetDeletionExact {
                thought_id,
                expected_content,
                expected_annotations,
                expected_deleted_at,
                expected_position,
                deleted_at,
                position,
            } => self.set_deletion_exact(
                *thought_id,
                expected_content,
                expected_annotations,
                *expected_deleted_at,
                *expected_position,
                *deleted_at,
                *position,
                at,
            )?,
            BoardMutation::MoveThought {
                thought_id,
                from,
                to,
            } => self.move_thought(*thought_id, *from, *to, at)?,
            BoardMutation::ReplaceContent {
                thought_id,
                before_content,
                before_annotations,
                after_content,
                after_annotations,
            } => self.replace_content(
                *thought_id,
                before_content,
                before_annotations,
                after_content,
                after_annotations,
                at,
            )?,
            BoardMutation::SetPresentation {
                thought_id,
                presentation,
            } => {
                let thought = self
                    .thought_mut(*thought_id)
                    .ok_or(DomainError::ThoughtNotFound(*thought_id))?;
                thought.presentation = *presentation;
                thought.updated_at = at;
            }
            BoardMutation::LegacySetCollapsed {
                thought_id,
                collapsed,
            } => {
                let thought = self
                    .thought_mut(*thought_id)
                    .ok_or(DomainError::ThoughtNotFound(*thought_id))?;
                thought.presentation = if *collapsed {
                    ThoughtPresentation::Collapsed
                } else {
                    ThoughtPresentation::Automatic
                };
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
        self.session.validate()?;
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
}
