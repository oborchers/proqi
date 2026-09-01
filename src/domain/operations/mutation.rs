//! Exact mutation mechanics for the session-board aggregate.

use super::{
    DomainError, SessionBoard, Thought, ThoughtId, ThoughtPosition, Timestamp, validate_annotations,
};
use crate::domain::ContentAnnotation;

impl SessionBoard {
    pub(super) fn add_or_restore(
        &mut self,
        mut thought: Thought,
        at: Timestamp,
    ) -> Result<(), DomainError> {
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

    pub(super) fn set_deletion(
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

    #[expect(
        clippy::too_many_arguments,
        reason = "the exact deletion transition carries its complete durable precondition"
    )]
    pub(super) fn set_deletion_exact(
        &mut self,
        thought_id: ThoughtId,
        expected_content: &str,
        expected_annotations: &[ContentAnnotation],
        expected_deleted_at: Option<Timestamp>,
        expected_position: ThoughtPosition,
        deleted_at: Option<Timestamp>,
        position: ThoughtPosition,
        at: Timestamp,
    ) -> Result<(), DomainError> {
        let thought = self
            .thought(thought_id)
            .ok_or(DomainError::ThoughtNotFound(thought_id))?;
        if thought.content != expected_content
            || thought.annotations != expected_annotations
            || thought.deleted_at != expected_deleted_at
            || thought.position != expected_position
        {
            return Err(DomainError::ThoughtContentConflict(thought_id));
        }
        self.set_deletion(thought_id, deleted_at, position, at)
    }

    pub(super) fn move_thought(
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

    pub(super) fn replace_content(
        &mut self,
        thought_id: ThoughtId,
        before_content: &str,
        before_annotations: &[ContentAnnotation],
        after_content: &str,
        after_annotations: &[ContentAnnotation],
        at: Timestamp,
    ) -> Result<(), DomainError> {
        validate_annotations(after_content, after_annotations)?;
        let thought = self
            .thought_mut(thought_id)
            .filter(|thought| thought.is_live())
            .ok_or(DomainError::ThoughtNotFound(thought_id))?;
        if thought.content != before_content || thought.annotations != before_annotations {
            return Err(DomainError::ThoughtContentConflict(thought_id));
        }
        after_content.clone_into(&mut thought.content);
        after_annotations.clone_into(&mut thought.annotations);
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
