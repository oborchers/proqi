//! Thought mutations routed through the reducer and durable store.

use crate::{
    application::{Action, reduce},
    domain::{BoardOperationKind, ContentAnnotation, OperationId, SessionId, ThoughtId, UndoScope},
    ports::{
        control::ControlMutation,
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{CommitReceipt, Store},
    },
};

use super::{SessionService, SessionServiceError, ThoughtMutation, match_replay};

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Add exact content at an optional live position.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, reducer, idempotency, or persistence failure.
    pub fn add_thought(
        &mut self,
        session_id: SessionId,
        content: String,
        position: Option<usize>,
        supplied_operation: Option<OperationId>,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        self.add_thought_with_annotations(
            session_id,
            content,
            Vec::new(),
            position,
            supplied_operation,
            false,
        )
    }

    /// Preserve exact content and already-valid Proqi presentation annotations.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, reducer, idempotency, annotation, or persistence failure.
    pub(crate) fn preserve_thought(
        &mut self,
        session_id: SessionId,
        content: String,
        annotations: Vec<ContentAnnotation>,
        position: Option<usize>,
        supplied_operation: Option<OperationId>,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        self.add_thought_with_annotations(
            session_id,
            content,
            annotations,
            position,
            supplied_operation,
            true,
        )
    }

    fn add_thought_with_annotations(
        &mut self,
        session_id: SessionId,
        content: String,
        annotations: Vec<ContentAnnotation>,
        position: Option<usize>,
        supplied_operation: Option<OperationId>,
        preserve: bool,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let thought_id = if supplied_operation.is_some() {
            ThoughtId::from_database_bytes(operation_id.database_bytes())
                .map_err(|_| SessionServiceError::IdempotencyConflict)?
        } else {
            self.ids.thought_id()
        };
        let replay = if preserve {
            ControlMutation::PreserveAdd {
                operation_id,
                thought_id,
                content: content.clone(),
                annotations: annotations.clone(),
                position,
            }
        } else {
            ControlMutation::Add {
                operation_id,
                thought_id,
                content: content.clone(),
                annotations: Vec::new(),
                position,
            }
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_thought(&existing, session_id, &replay);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_thought(&existing, session_id, &replay);
        }
        let mut state = self.load_live_state(session_id)?;
        let action = if preserve {
            Action::CreateOwnedThought(crate::application::OwnedThoughtCreation::preserved(
                thought_id,
                operation_id,
                content,
                annotations,
                position,
                self.clock.now(),
            ))
        } else {
            Action::CreateThought {
                thought_id,
                operation_id,
                content,
                annotations,
                insertion_index: position,
                at: self.clock.now(),
            }
        };
        let effects = reduce(&mut state, action)?;
        let receipt = self.commit_single_effect(&effects)?;
        Ok(ThoughtMutation {
            thought_id,
            receipt,
        })
    }

    /// Soft-delete one live thought as a reversible board operation.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, absence, reducer, idempotency, or persistence failure.
    pub fn delete_thought(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        supplied_operation: Option<OperationId>,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let replay = ControlMutation::Delete {
            operation_id,
            thought_id,
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_thought(&existing, session_id, &replay);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_thought(&existing, session_id, &replay);
        }
        let mut state = self.load_live_state(session_id)?;
        let effects = reduce(
            &mut state,
            Action::DeleteThought {
                operation_id,
                thought_id,
                kind: BoardOperationKind::Delete,
                at: self.clock.now(),
            },
        )?;
        let receipt = self.commit_single_effect(&effects)?;
        Ok(ThoughtMutation {
            thought_id,
            receipt,
        })
    }

    /// Move one live thought to a zero-based position.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, position, reducer, idempotency, or persistence failure.
    pub fn move_thought(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        position: usize,
        supplied_operation: Option<OperationId>,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let replay = ControlMutation::Move {
            operation_id,
            thought_id,
            position,
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_thought(&existing, session_id, &replay);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_thought(&existing, session_id, &replay);
        }
        let mut state = self.load_live_state(session_id)?;
        let effects = reduce(
            &mut state,
            Action::MoveThought {
                operation_id,
                thought_id,
                to: position,
                at: self.clock.now(),
            },
        )?;
        let receipt = self.commit_single_effect(&effects)?;
        Ok(ThoughtMutation {
            thought_id,
            receipt,
        })
    }

    /// Persistently undo or redo one history scope.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, history, idempotency, reducer, or persistence failure.
    pub fn move_history(
        &mut self,
        session_id: SessionId,
        scope: UndoScope,
        undo: bool,
        supplied_operation: Option<OperationId>,
    ) -> Result<CommitReceipt, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let replay = ControlMutation::History {
            operation_id,
            scope,
            undo,
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return Ok(match_replay(&existing, session_id, &replay)?.durable);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return Ok(match_replay(&existing, session_id, &replay)?.durable);
        }
        let mut state = self.load_live_state(session_id)?;
        let action = if undo {
            Action::Undo {
                operation_id,
                scope,
                at: self.clock.now(),
            }
        } else {
            Action::Redo {
                operation_id,
                scope,
                at: self.clock.now(),
            }
        };
        let effects = reduce(&mut state, action)?;
        self.commit_single_effect(&effects)
    }
}

fn match_existing_thought(
    existing: &crate::ports::store::StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> Result<ThoughtMutation, SessionServiceError> {
    let thought_id = mutation
        .thought_id()
        .ok_or(SessionServiceError::IdempotencyConflict)?;
    let receipt = match_replay(existing, session_id, mutation)?;
    Ok(ThoughtMutation {
        thought_id,
        receipt: receipt.durable,
    })
}
