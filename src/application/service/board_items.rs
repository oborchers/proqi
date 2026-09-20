//! Mixed Board-item mutations routed through the canonical reducer and history.

use crate::{
    application::{Action, reduce},
    domain::{BoardItemId, BoardOperationKind, OperationId, SeparatorId, SessionId, ThoughtId},
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
    /// Insert one payload-free separator at a shared Board position.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, reducer, idempotency, or persistence failure.
    pub fn insert_separator(
        &mut self,
        session_id: SessionId,
        position: Option<usize>,
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let separator_id = SeparatorId::from_database_bytes(operation_id.database_bytes())
            .map_err(|_| SessionServiceError::IdempotencyConflict)?;
        let mutation = ControlMutation::InsertSeparator {
            operation_id,
            separator_id,
            position,
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing(&existing, session_id, &mutation);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing(&existing, session_id, &mutation);
        }
        let mut state = self.load_live_state(session_id)?;
        let insertion_index = position.unwrap_or_else(|| state.board.live_items().len());
        let effects = reduce(
            &mut state,
            Action::InsertSeparator {
                separator_id,
                operation_id,
                insertion_index,
                at: self.clock.now(),
            },
        )?;
        let receipt = self.commit_control_effects(effects, session_id, &mutation)?;
        Ok(BoardItemMutation {
            item_ids: vec![separator_id.into()],
            receipt,
        })
    }

    /// Move one typed live item within the shared Board order.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, reducer, idempotency, or persistence failure.
    pub fn move_item(
        &mut self,
        session_id: SessionId,
        item_id: BoardItemId,
        position: usize,
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let mutation = ControlMutation::MoveItem {
            operation_id,
            item_id,
            position,
        };
        self.apply_item_action(
            session_id,
            &mutation,
            Action::MoveItem {
                operation_id,
                item_id,
                to: position,
                at: self.clock.now(),
            },
            vec![item_id],
        )
    }

    /// Soft-delete typed live items as one reversible Board operation.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, reducer, idempotency, or persistence failure.
    pub fn delete_items(
        &mut self,
        session_id: SessionId,
        item_ids: Vec<BoardItemId>,
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        let mutation = ControlMutation::DeleteItems {
            operation_id,
            item_ids: item_ids.clone(),
        };
        self.apply_item_action(
            session_id,
            &mutation,
            Action::DeleteItems {
                operation_id,
                item_ids: item_ids.clone(),
                kind: BoardOperationKind::Delete,
                at: self.clock.now(),
            },
            item_ids,
        )
    }

    /// Duplicate typed live items below their exact Board-ordered source range.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, reducer, idempotency, or persistence failure.
    pub fn duplicate_items(
        &mut self,
        session_id: SessionId,
        item_ids: Vec<BoardItemId>,
        supplied_operation: Option<OperationId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = supplied_operation.unwrap_or_else(|| self.ids.operation_id());
        if let Some(existing) = self.store.operation_request(operation_id)? {
            let mutation = ControlMutation::DuplicateItems {
                operation_id,
                item_ids,
                duplicate_ids: Vec::new(),
            };
            return match_existing(&existing, session_id, &mutation);
        }
        let duplicate_ids = derived_duplicate_item_ids(operation_id, &item_ids)?;
        let mutation = ControlMutation::DuplicateItems {
            operation_id,
            item_ids: item_ids.clone(),
            duplicate_ids: duplicate_ids.clone(),
        };
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing(&existing, session_id, &mutation);
        }
        let mut state = self.load_live_state(session_id)?;
        let effects = reduce(
            &mut state,
            Action::DuplicateItems {
                operation_id,
                item_ids,
                duplicate_ids: duplicate_ids.clone(),
                at: self.clock.now(),
            },
        )?;
        let receipt = self.commit_control_effects(effects, session_id, &mutation)?;
        Ok(BoardItemMutation {
            item_ids: duplicate_ids,
            receipt,
        })
    }

    fn apply_item_action(
        &mut self,
        session_id: SessionId,
        mutation: &ControlMutation,
        action: Action,
        item_ids: Vec<BoardItemId>,
    ) -> Result<BoardItemMutation, SessionServiceError> {
        let operation_id = mutation
            .durable_operation_id()
            .ok_or(SessionServiceError::IdempotencyConflict)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing(&existing, session_id, mutation);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing(&existing, session_id, mutation);
        }
        let mut state = self.load_live_state(session_id)?;
        let effects = reduce(&mut state, action)?;
        let receipt = self.commit_control_effects(effects, session_id, mutation)?;
        Ok(BoardItemMutation { item_ids, receipt })
    }
}

pub(crate) fn derived_duplicate_item_ids(
    operation_id: OperationId,
    sources: &[BoardItemId],
) -> Result<Vec<BoardItemId>, SessionServiceError> {
    use sha2::{Digest as _, Sha256};

    sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let ordinal =
                u64::try_from(index).map_err(|_| SessionServiceError::IdempotencyConflict)?;
            let mut hasher = Sha256::new();
            hasher.update(operation_id.database_bytes());
            hasher.update(ordinal.to_be_bytes());
            match source {
                BoardItemId::Thought(id) => {
                    hasher.update(b"thought");
                    hasher.update(id.database_bytes());
                }
                BoardItemId::Separator(id) => {
                    hasher.update(b"separator");
                    hasher.update(id.database_bytes());
                }
            }
            let digest = hasher.finalize();
            let mut bytes = operation_id.database_bytes();
            bytes[6..].copy_from_slice(&digest[..10]);
            bytes[6] = (bytes[6] & 0x0f) | 0x70;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            match source {
                BoardItemId::Thought(_) => ThoughtId::from_database_bytes(bytes)
                    .map(BoardItemId::Thought)
                    .map_err(|_| SessionServiceError::IdempotencyConflict),
                BoardItemId::Separator(_) => SeparatorId::from_database_bytes(bytes)
                    .map(BoardItemId::Separator)
                    .map_err(|_| SessionServiceError::IdempotencyConflict),
            }
        })
        .collect()
}

fn match_existing(
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

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use crate::domain::{BoardItemId, OperationId, SeparatorId, ThoughtId};

    use super::derived_duplicate_item_ids;

    #[test]
    fn duplicate_id_derivation_is_platform_stable_and_type_preserving() {
        let operation =
            OperationId::from_str("op_06g30t8fudrq55fdkjqr6mpe44").expect("operation fixture");
        let thought =
            ThoughtId::from_str("tht_06g30t8fudrq55fdkk348i7388").expect("thought fixture");
        let separator =
            SeparatorId::from_str("sep_06g30t8fudrq55fdkjqr6mpe44").expect("separator fixture");

        let derived = derived_duplicate_item_ids(
            operation,
            &[
                BoardItemId::Thought(thought),
                BoardItemId::Separator(separator),
            ],
        )
        .expect("derived identities");

        assert_eq!(
            derived[0],
            BoardItemId::Thought(
                ThoughtId::from_str("tht_06g30t8fudqjv6qhufgfhh4veg")
                    .expect("derived thought fixture"),
            )
        );
        assert_eq!(
            derived[1],
            BoardItemId::Separator(
                SeparatorId::from_str("sep_06g30t8fudv8d8p5fklmlpsvek")
                    .expect("derived separator fixture"),
            )
        );
    }
}
