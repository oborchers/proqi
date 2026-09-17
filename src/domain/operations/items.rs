//! Canonical ordered Board item projection and aggregate validation.

use std::collections::HashSet;

use super::{DomainError, Separator, SeparatorId, SessionBoard};
use crate::domain::{BoardItemId, BoardItemRef, ThoughtPosition, validate_annotations};

impl SessionBoard {
    /// All separators, including recoverably deleted records.
    #[must_use]
    pub fn separators(&self) -> &[Separator] {
        &self.separators
    }

    /// Live separators ordered by shared Board position.
    #[must_use]
    pub fn live_separators(&self) -> Vec<&Separator> {
        let mut separators: Vec<_> = self
            .separators
            .iter()
            .filter(|separator| separator.is_live())
            .collect();
        separators.sort_by_key(|separator| separator.position);
        separators
    }

    /// Every live thought and separator in one normalized Board order.
    #[must_use]
    pub fn live_items(&self) -> Vec<BoardItemRef<'_>> {
        let mut items = Vec::with_capacity(self.thoughts.len() + self.separators.len());
        items.extend(
            self.thoughts
                .iter()
                .filter(|thought| thought.is_live())
                .map(BoardItemRef::Thought),
        );
        items.extend(
            self.separators
                .iter()
                .filter(|separator| separator.is_live())
                .map(BoardItemRef::Separator),
        );
        items.sort_by_key(|item| item.position());
        items
    }

    /// Look up any retained separator.
    #[must_use]
    pub fn separator(&self, id: SeparatorId) -> Option<&Separator> {
        self.separators.iter().find(|separator| separator.id == id)
    }

    /// Look up any retained separator mutably.
    pub fn separator_mut(&mut self, id: SeparatorId) -> Option<&mut Separator> {
        self.separators
            .iter_mut()
            .find(|separator| separator.id == id)
    }

    /// Look up one live item's shared position.
    #[must_use]
    pub fn item_position(&self, id: BoardItemId) -> Option<ThoughtPosition> {
        match id {
            BoardItemId::Thought(id) => self
                .thought(id)
                .filter(|item| item.is_live())
                .map(|item| item.position),
            BoardItemId::Separator(id) => self
                .separator(id)
                .filter(|item| item.is_live())
                .map(|item| item.position),
        }
    }

    /// Validate ownership and normalized live ordering.
    ///
    /// # Errors
    ///
    /// Returns a domain error for duplicate identities, wrong ownership, or invalid positions.
    pub fn validate(&self) -> Result<(), DomainError> {
        self.session.validate()?;
        self.validate_thoughts()?;
        self.validate_separators()?;
        for (expected, item) in self.live_items().into_iter().enumerate() {
            if usize::try_from(item.position().get()).ok() != Some(expected) {
                return Err(DomainError::NonNormalizedPositions);
            }
        }
        Ok(())
    }

    fn validate_thoughts(&self) -> Result<(), DomainError> {
        let mut identities = HashSet::with_capacity(self.thoughts.len());
        let mut attachment_ids = HashSet::new();
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
            if thought.is_live() {
                super::super::attachment_numbering::validate_unique(
                    &thought.annotations,
                    &mut attachment_ids,
                )?;
            }
        }
        Ok(())
    }

    fn validate_separators(&self) -> Result<(), DomainError> {
        let mut identities = HashSet::with_capacity(self.separators.len());
        for separator in &self.separators {
            if !identities.insert(separator.id) {
                return Err(DomainError::DuplicateSeparatorId(separator.id));
            }
            if separator.session_id != self.session.id {
                return Err(DomainError::WrongSeparatorSession {
                    separator_id: separator.id,
                    session_id: self.session.id,
                });
            }
        }
        Ok(())
    }
}
