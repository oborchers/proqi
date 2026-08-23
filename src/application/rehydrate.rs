//! Reconstruct reducer-owned history from a durable snapshot.

use std::collections::{HashMap, HashSet};

use super::model::{AppState, ApplicationError, ApplicationResult, EditorHistory};
use crate::{domain::ThoughtId, ports::store::SessionSnapshot};

impl AppState {
    /// Rehydrate current state and both persistent undo scopes from storage.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError::InvalidState`] when snapshot history does
    /// not belong to the board or a retained cursor is out of bounds.
    pub fn from_snapshot(snapshot: SessionSnapshot) -> ApplicationResult<Self> {
        let session_id = snapshot.board.session.id;
        if snapshot.board_history_cursor > snapshot.board_operations.len()
            || snapshot
                .board_operations
                .iter()
                .any(|operation| operation.session_id != session_id)
        {
            return Err(ApplicationError::InvalidState);
        }
        let mut histories: HashMap<ThoughtId, EditorHistory> = HashMap::new();
        for revision in snapshot.revisions {
            if revision.session_id != session_id
                || snapshot.board.thought(revision.thought_id).is_none()
            {
                return Err(ApplicationError::InvalidState);
            }
            histories
                .entry(revision.thought_id)
                .or_default()
                .revisions
                .push(revision);
        }
        let mut cursor_owners = HashSet::new();
        for (thought_id, cursor) in snapshot.editor_history_cursors {
            if !cursor_owners.insert(thought_id) || snapshot.board.thought(thought_id).is_none() {
                return Err(ApplicationError::InvalidState);
            }
            let history = histories.entry(thought_id).or_default();
            if cursor > history.revisions.len() {
                return Err(ApplicationError::InvalidState);
            }
            history.cursor = cursor;
        }
        let mut state = Self::new(snapshot.board);
        state.board_history = snapshot.board_operations;
        state.board_history_cursor = snapshot.board_history_cursor;
        state.editor_histories = histories;
        Ok(state)
    }
}
