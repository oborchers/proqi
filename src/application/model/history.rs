//! Canonical ordering between durable Board and Editor history owners.

use crate::domain::{BoardOperation, OperationSequence, ThoughtId, UndoScope};

use super::{AppState, InteractionMode};

impl AppState {
    /// Resolve a currently available durable history owner.
    #[must_use]
    pub fn history_scope(&self, mode: InteractionMode, undo: bool) -> Option<UndoScope> {
        match mode {
            InteractionMode::Board => self.board_operation(undo).map(|_| UndoScope::Board),
            InteractionMode::Compose => (!undo
                && self
                    .board_operation(false)
                    .is_some_and(|operation| operation.compose_handoff().is_some()))
            .then_some(UndoScope::Board),
            InteractionMode::Edit { thought_id } => {
                let editor = self.editor_sequence(thought_id, undo);
                let board = self.board_operation(undo).filter(|operation| {
                    operation.kind.belongs_to_thought_content()
                        && operation.addresses_thought(thought_id)
                });
                match (board, editor) {
                    (Some(operation), editor)
                        if board_precedes_editor(operation.sequence, editor, undo) =>
                    {
                        Some(UndoScope::Board)
                    }
                    (_, Some(_)) => Some(UndoScope::Editor { thought_id }),
                    _ => None,
                }
            }
        }
    }

    /// Resolve the next durable history owner in sequence order.
    #[must_use]
    pub fn preferred_undo_scope(&self, mode: InteractionMode) -> UndoScope {
        self.preferred_history_scope(mode, true)
    }

    /// Resolve the next durable redo owner in sequence order.
    #[must_use]
    pub fn preferred_redo_scope(&self, mode: InteractionMode) -> UndoScope {
        self.preferred_history_scope(mode, false)
    }

    fn preferred_history_scope(&self, mode: InteractionMode, undo: bool) -> UndoScope {
        if let Some(scope) = self.history_scope(mode, undo) {
            return scope;
        }
        let InteractionMode::Edit { thought_id } = mode else {
            return UndoScope::Board;
        };
        let editor_sequence = self.editor_sequence(thought_id, undo);
        let board = self.board_operation(undo).filter(|operation| {
            operation.kind.belongs_to_thought_content() && operation.addresses_thought(thought_id)
        });
        if board.is_some_and(|operation| {
            board_precedes_editor(operation.sequence, editor_sequence, undo)
        }) {
            UndoScope::Board
        } else {
            UndoScope::Editor { thought_id }
        }
    }

    fn editor_sequence(&self, thought_id: ThoughtId, undo: bool) -> Option<OperationSequence> {
        let history = self.editor_histories.get(&thought_id)?;
        let revision = if undo {
            history
                .cursor
                .checked_sub(1)
                .and_then(|index| history.revisions.get(index))
        } else {
            history.revisions.get(history.cursor)
        }?;
        Some(revision.sequence)
    }

    fn board_operation(&self, undo: bool) -> Option<&BoardOperation> {
        if undo {
            self.board_history_cursor
                .checked_sub(1)
                .and_then(|index| self.board_history.get(index))
        } else {
            self.board_history.get(self.board_history_cursor)
        }
    }

    pub(super) fn applied_compose_handoff(
        &self,
        thought_id: ThoughtId,
    ) -> Option<(
        crate::domain::TextPosition,
        Option<crate::domain::TextPosition>,
    )> {
        self.board_history[..self.board_history_cursor]
            .iter()
            .rev()
            .find_map(BoardOperation::compose_handoff)
            .and_then(|(id, cursor, anchor)| (id == thought_id).then_some((cursor, anchor)))
    }
}

fn board_precedes_editor(
    board: OperationSequence,
    editor: Option<OperationSequence>,
    undo: bool,
) -> bool {
    match editor {
        None => true,
        Some(editor) if undo => board > editor,
        Some(editor) => board < editor,
    }
}
