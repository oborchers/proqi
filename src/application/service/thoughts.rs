//! Thought mutations routed through the reducer and durable store.

use crate::{
    application::{Action, AppState, Effect, reduce},
    domain::{BoardMutation, BoardOperationKind, OperationId, SessionId, ThoughtId, UndoScope},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{CommitReceipt, OperationBatch, Store, StoredOperationRequest},
    },
};

use super::{SessionService, SessionServiceError, ThoughtMutation};

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
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let thought_id = if supplied_operation.is_some() {
            ThoughtId::from_database_bytes(operation_id.database_bytes())
                .map_err(|_| SessionServiceError::IdempotencyConflict)?
        } else {
            self.ids.thought_id()
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_add(&existing, session_id, thought_id, &content, position);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        let mut state = self.load_live_state(session_id)?;
        let effects = reduce(
            &mut state,
            Action::CreateThought {
                thought_id,
                operation_id,
                content,
                insertion_index: position,
                at: self.clock.now(),
            },
        )?;
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
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_delete(&existing, session_id, thought_id);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
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
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_move(&existing, session_id, thought_id, position);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
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
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_history(&existing, session_id, scope, undo);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
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

    fn load_live_state(&mut self, id: SessionId) -> Result<AppState, SessionServiceError> {
        let snapshot = self.store.load_session(id)?;
        if snapshot.board.session.deleted_at.is_some() {
            return Err(SessionServiceError::SessionTrashed(id));
        }
        Ok(AppState::from_snapshot(snapshot)?)
    }

    fn commit_single_effect(
        &mut self,
        effects: &[Effect],
    ) -> Result<CommitReceipt, SessionServiceError> {
        let [effect] = effects else {
            return Err(SessionServiceError::NoDurableMutation);
        };
        let batch = match effect {
            Effect::CommitBoardOperation(operation) => OperationBatch::Board(operation.clone()),
            Effect::CommitHistoryMove {
                operation_id,
                session_id,
                scope,
                undo,
                sequence,
                at,
            } => OperationBatch::HistoryMove {
                operation_id: *operation_id,
                session_id: *session_id,
                scope: *scope,
                undo: *undo,
                sequence: *sequence,
                at: *at,
            },
            _ => return Err(SessionServiceError::NoDurableMutation),
        };
        self.store
            .commit(&batch)?
            .ok_or(SessionServiceError::NoDurableMutation)
    }
}

fn match_existing_add(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    thought_id: ThoughtId,
    content: &str,
    position: Option<usize>,
) -> Result<ThoughtMutation, SessionServiceError> {
    let StoredOperationRequest::Board { operation, receipt } = existing else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    let BoardMutation::AddThought { thought } = &operation.forward else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    let expected_position = position.and_then(|value| u32::try_from(value).ok());
    if operation.session_id != session_id
        || operation.kind != BoardOperationKind::Create
        || thought.id != thought_id
        || thought.content != content
        || expected_position.is_some_and(|value| thought.position.get() != value)
    {
        return Err(SessionServiceError::IdempotencyConflict);
    }
    Ok(ThoughtMutation {
        thought_id,
        receipt: *receipt,
    })
}

fn match_existing_delete(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    thought_id: ThoughtId,
) -> Result<ThoughtMutation, SessionServiceError> {
    let StoredOperationRequest::Board { operation, receipt } = existing else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    let BoardMutation::SetDeletion {
        thought_id: stored,
        deleted_at: Some(_),
        ..
    } = &operation.forward
    else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    if operation.session_id != session_id
        || operation.kind != BoardOperationKind::Delete
        || *stored != thought_id
    {
        return Err(SessionServiceError::IdempotencyConflict);
    }
    Ok(ThoughtMutation {
        thought_id,
        receipt: *receipt,
    })
}

fn match_existing_move(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    thought_id: ThoughtId,
    position: usize,
) -> Result<ThoughtMutation, SessionServiceError> {
    let StoredOperationRequest::Board { operation, receipt } = existing else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    let BoardMutation::MoveThought {
        thought_id: stored,
        to,
        ..
    } = &operation.forward
    else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    if operation.session_id != session_id
        || operation.kind != BoardOperationKind::Reorder
        || *stored != thought_id
        || usize::try_from(to.get()).ok() != Some(position)
    {
        return Err(SessionServiceError::IdempotencyConflict);
    }
    Ok(ThoughtMutation {
        thought_id,
        receipt: *receipt,
    })
}

fn match_existing_history(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    scope: UndoScope,
    undo: bool,
) -> Result<CommitReceipt, SessionServiceError> {
    let StoredOperationRequest::HistoryMove {
        session_id: stored_session,
        scope: stored_scope,
        undo: stored_undo,
        receipt,
    } = existing
    else {
        return Err(SessionServiceError::IdempotencyConflict);
    };
    if *stored_session != session_id || *stored_scope != scope || *stored_undo != undo {
        return Err(SessionServiceError::IdempotencyConflict);
    }
    Ok(*receipt)
}
