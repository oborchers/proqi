//! Board and editor history inspection and preferred-scope policy.

use crate::domain::{
    BoardOperation, BoardOperationKind, TextPosition, ThoughtId, ThoughtRevision, UndoScope,
};

use super::{AppState, InteractionMode};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::application) struct EditorHistory {
    pub(in crate::application) revisions: Vec<ThoughtRevision>,
    pub(in crate::application) cursor: usize,
}

impl AppState {
    /// Board history entries retained for undo and redo.
    #[must_use]
    pub fn board_history(&self) -> &[BoardOperation] {
        &self.board_history
    }

    /// Number of currently applied board history entries.
    #[must_use]
    pub const fn board_history_cursor(&self) -> usize {
        self.board_history_cursor
    }

    /// Number of currently applied revisions for one thought.
    #[must_use]
    pub fn editor_history_cursor(&self, thought_id: ThoughtId) -> usize {
        self.editor_histories
            .get(&thought_id)
            .map_or(0, |history| history.cursor)
    }

    /// Restore the logical cursor represented by the currently applied revision prefix.
    #[must_use]
    pub fn restored_editor_cursor(&self, thought_id: ThoughtId) -> Option<TextPosition> {
        let history = self.editor_histories.get(&thought_id)?;
        if history.cursor == 0 {
            history
                .revisions
                .first()
                .map(|revision| revision.before_cursor)
        } else {
            history
                .revisions
                .get(history.cursor - 1)
                .map(|revision| revision.after_cursor)
        }
    }

    /// Choose a transformation as the next undo unit when it is newer than the
    /// active thought's latest editor revision and directly owns that thought.
    #[must_use]
    pub fn preferred_undo_scope(&self, mode: InteractionMode) -> UndoScope {
        let InteractionMode::Edit { thought_id } = mode else {
            return UndoScope::Board;
        };
        let editor_sequence = self
            .editor_histories
            .get(&thought_id)
            .and_then(|history| {
                history
                    .cursor
                    .checked_sub(1)
                    .and_then(|index| history.revisions.get(index))
            })
            .map(|revision| revision.sequence);
        let transformation = self
            .board_history_cursor
            .checked_sub(1)
            .and_then(|index| self.board_history.get(index))
            .filter(|operation| {
                matches!(
                    operation.kind,
                    BoardOperationKind::Split
                        | BoardOperationKind::Extract
                        | BoardOperationKind::Merge
                        | BoardOperationKind::Reflow
                ) && operation.forward.addresses(thought_id)
            });
        if transformation.is_some_and(|operation| {
            editor_sequence.is_none_or(|sequence| operation.sequence > sequence)
        }) {
            UndoScope::Board
        } else {
            UndoScope::Editor { thought_id }
        }
    }

    /// Choose a transformation as the next redo unit when it precedes the
    /// active thought's next editor revision and directly owns that thought.
    #[must_use]
    pub fn preferred_redo_scope(&self, mode: InteractionMode) -> UndoScope {
        let InteractionMode::Edit { thought_id } = mode else {
            return UndoScope::Board;
        };
        let editor_sequence = self
            .editor_histories
            .get(&thought_id)
            .and_then(|history| history.revisions.get(history.cursor))
            .map(|revision| revision.sequence);
        let transformation =
            self.board_history
                .get(self.board_history_cursor)
                .filter(|operation| {
                    matches!(
                        operation.kind,
                        BoardOperationKind::Split
                            | BoardOperationKind::Extract
                            | BoardOperationKind::Merge
                            | BoardOperationKind::Reflow
                    ) && operation.forward.addresses(thought_id)
                });
        if transformation.is_some_and(|operation| {
            editor_sequence.is_none_or(|sequence| operation.sequence < sequence)
        }) {
            UndoScope::Board
        } else {
            UndoScope::Editor { thought_id }
        }
    }

    /// Whether the current interaction owner has one applicable undo unit.
    #[must_use]
    pub(crate) fn can_undo(&self, mode: InteractionMode) -> bool {
        if matches!(mode, InteractionMode::Compose) {
            return false;
        }
        match self.preferred_undo_scope(mode) {
            UndoScope::Board => self.board_history_cursor > 0,
            UndoScope::Editor { thought_id } => self
                .editor_histories
                .get(&thought_id)
                .is_some_and(|history| history.cursor > 0),
        }
    }

    /// Whether the current interaction owner has one applicable redo unit.
    #[must_use]
    pub(crate) fn can_redo(&self, mode: InteractionMode) -> bool {
        if matches!(mode, InteractionMode::Compose) {
            return false;
        }
        match self.preferred_redo_scope(mode) {
            UndoScope::Board => self.board_history_cursor < self.board_history.len(),
            UndoScope::Editor { thought_id } => self
                .editor_histories
                .get(&thought_id)
                .is_some_and(|history| history.cursor < history.revisions.len()),
        }
    }
}
