//! Scriptable thought transformations through canonical Board operations.

use crate::{
    application::{Action, exact_live_thought, reduce},
    domain::{OperationId, SessionId, ThoughtId},
    ports::{
        control::ControlMutation,
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::Store,
    },
};

use super::{BoardItemMutation, SessionService, SessionServiceError, match_replay};

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Split one exact thought at a UTF-8 byte boundary.
    ///
    /// # Errors
    ///
    /// Returns a typed precondition, reducer, idempotency, or persistence failure.
    pub fn split_thought(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        at_byte: usize,
        expected_digest: [u8; 32],
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let new_thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
            .map_err(|_| SessionServiceError::IdempotencyConflict)?;
        let mutation = ControlMutation::SplitThought {
            operation_id,
            thought_id,
            new_thought_id,
            expected_digest,
            at_byte,
        };
        self.apply_transform(session_id, &mutation, |state, at| {
            let source = exact_live_thought(state, thought_id, Some(expected_digest))?;
            Ok(Action::SplitThought {
                thought_id,
                new_thought_id,
                operation_id,
                expected_content: source.content.clone(),
                expected_annotations: source.annotations.clone(),
                source_content: source.content.clone(),
                source_annotations: source.annotations.clone(),
                at_byte,
                at,
            })
        })
    }

    /// Extract one exact nonempty UTF-8 byte range into a new neighboring thought.
    ///
    /// # Errors
    ///
    /// Returns a typed precondition, reducer, idempotency, or persistence failure.
    pub fn extract_thought(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        range: std::ops::Range<usize>,
        expected_digest: [u8; 32],
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let new_thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes())
            .map_err(|_| SessionServiceError::IdempotencyConflict)?;
        let mutation = ControlMutation::ExtractThought {
            operation_id,
            thought_id,
            new_thought_id,
            expected_digest,
            start_byte: range.start,
            end_byte: range.end,
        };
        self.apply_transform(session_id, &mutation, |state, at| {
            let source = exact_live_thought(state, thought_id, Some(expected_digest))?;
            Ok(Action::ExtractThought {
                thought_id,
                new_thought_id,
                operation_id,
                expected_content: source.content.clone(),
                expected_annotations: source.annotations.clone(),
                source_content: source.content.clone(),
                source_annotations: source.annotations.clone(),
                range,
                at,
            })
        })
    }

    /// Merge exact Board-contiguous thoughts with one validated configured separator.
    ///
    /// # Errors
    ///
    /// Returns a typed precondition, contiguity, idempotency, or persistence failure.
    pub fn merge_thoughts(
        &mut self,
        session_id: SessionId,
        thought_ids: Vec<ThoughtId>,
        expected_digests: &[[u8; 32]],
        separator: String,
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let mutation = ControlMutation::MergeThoughts {
            operation_id,
            thought_ids: thought_ids.clone(),
            expected_digests: expected_digests.to_vec(),
            separator: separator.clone(),
        };
        self.apply_transform(session_id, &mutation, |state, at| {
            if thought_ids.len() != expected_digests.len() {
                return Err(crate::application::ApplicationError::InvalidState.into());
            }
            let expected_sources = thought_ids
                .iter()
                .zip(expected_digests)
                .map(|(id, digest)| exact_live_thought(state, *id, Some(*digest)).cloned())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Action::MergeThoughts {
                operation_id,
                thought_ids,
                expected_sources,
                separator,
                at,
            })
        })
    }

    /// Clean one exact thought with the canonical annotation-safe spacing policy.
    ///
    /// # Errors
    ///
    /// Returns a typed precondition, no-change, idempotency, or persistence failure.
    pub fn reflow_thought(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        expected_digest: [u8; 32],
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let mutation = ControlMutation::ReflowThought {
            operation_id,
            thought_id,
            expected_digest,
        };
        self.apply_transform(session_id, &mutation, |state, at| {
            let source = exact_live_thought(state, thought_id, Some(expected_digest))?;
            let projection =
                crate::application::text_reflow::reflow(&source.content, &source.annotations)
                    .map_err(|()| crate::application::ApplicationError::InvalidState)?;
            let (content, annotations) = match projection.outcome {
                crate::application::text_reflow::TextReflowOutcome::Changed {
                    content,
                    annotations,
                } => (content, annotations),
                crate::application::text_reflow::TextReflowOutcome::Unchanged
                | crate::application::text_reflow::TextReflowOutcome::Empty => {
                    return Err(SessionServiceError::NoDurableMutation);
                }
            };
            Ok(Action::ReflowThought(
                crate::application::OwnedThoughtReflow {
                    thought_id,
                    operation_id,
                    before_content: source.content.clone(),
                    before_annotations: source.annotations.clone(),
                    after_content: content,
                    after_annotations: annotations,
                    at,
                },
            ))
        })
    }

    pub(super) fn apply_transform(
        &mut self,
        session_id: SessionId,
        mutation: &ControlMutation,
        action: impl FnOnce(
            &crate::application::AppState,
            crate::domain::Timestamp,
        ) -> Result<Action, SessionServiceError>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = mutation
            .durable_operation_id()
            .ok_or(SessionServiceError::IdempotencyConflict)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return replay(&existing, session_id, mutation);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return replay(&existing, session_id, mutation);
        }
        let mut state = self.load_live_state(session_id)?;
        let action = action(&state, self.clock.now())?;
        let effects = reduce(&mut state, action)?;
        let receipt = self.commit_control_effects(effects, session_id, mutation)?;
        Ok(BoardItemMutation {
            item_ids: mutation.item_ids(),
            receipt,
        })
    }
}

fn replay(
    existing: &crate::ports::store::StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> Result<BoardItemMutation, SessionServiceError> {
    let receipt = match_replay(existing, session_id, mutation)?;
    Ok(BoardItemMutation {
        item_ids: receipt.item_ids,
        receipt: receipt.durable,
    })
}
