//! Canonical ordering between durable Board and Editor history owners.

use crate::domain::{BoardOperation, OperationSequence, ThoughtId, UndoScope};

use super::{AppState, HistoryResolution, InteractionMode};
use crate::application::{ApplicationError, ApplicationResult};

impl AppState {
    /// Resolve a currently available durable history owner.
    #[must_use]
    pub fn history_scope(&self, mode: InteractionMode, undo: bool) -> Option<UndoScope> {
        match self.history_resolution(mode, undo) {
            HistoryResolution::Ready(scope) => Some(scope),
            HistoryResolution::Empty
            | HistoryResolution::BlockedByEditor { .. }
            | HistoryResolution::BlockedByBoard { .. } => None,
        }
    }

    /// Resolve availability and causal ordering for the active durable history owner.
    #[must_use]
    pub fn history_resolution(&self, mode: InteractionMode, undo: bool) -> HistoryResolution {
        match mode {
            InteractionMode::Board => self.resolve_board_operation(undo),
            InteractionMode::Compose => self.resolve_compose_redo(undo),
            InteractionMode::Edit { thought_id } => self.resolve_edit_operation(thought_id, undo),
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
        match self.history_resolution(mode, undo) {
            HistoryResolution::Ready(scope) => scope,
            HistoryResolution::BlockedByEditor { thought_id }
            | HistoryResolution::BlockedByBoard { thought_id } => UndoScope::Editor { thought_id },
            HistoryResolution::Empty => match mode {
                InteractionMode::Edit { thought_id } => UndoScope::Editor { thought_id },
                InteractionMode::Board | InteractionMode::Compose => UndoScope::Board,
            },
        }
    }

    pub(crate) fn ensure_history_scope_ready(
        &self,
        scope: UndoScope,
        undo: bool,
    ) -> ApplicationResult<()> {
        match scope {
            UndoScope::Board => {
                let operation = self
                    .board_operation(undo)
                    .ok_or(ApplicationError::InvalidState)?;
                self.board_dependency_blocker(operation, undo)
                    .map_or(Ok(()), |thought_id| {
                        Err(ApplicationError::HistoryDependency(thought_id))
                    })
            }
            UndoScope::Editor { thought_id } => {
                let editor = self
                    .editor_sequence(thought_id, undo)
                    .ok_or(ApplicationError::InvalidState)?;
                let board_must_move_first = self.board_operation(undo).is_some_and(|operation| {
                    operation.kind.belongs_to_thought_content()
                        && operation.addresses_thought(thought_id)
                        && board_precedes_editor(operation.sequence, Some(editor), undo)
                });
                if board_must_move_first {
                    Err(ApplicationError::HistoryDependency(thought_id))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn resolve_board_operation(&self, undo: bool) -> HistoryResolution {
        let Some(operation) = self.board_operation(undo) else {
            return HistoryResolution::Empty;
        };
        self.board_dependency_blocker(operation, undo)
            .map_or(HistoryResolution::Ready(UndoScope::Board), |thought_id| {
                HistoryResolution::BlockedByEditor { thought_id }
            })
    }

    fn resolve_compose_redo(&self, undo: bool) -> HistoryResolution {
        let Some(operation) = (!undo)
            .then(|| self.board_operation(false))
            .flatten()
            .filter(|operation| operation.compose_handoff().is_some())
        else {
            return HistoryResolution::Empty;
        };
        self.board_dependency_blocker(operation, false)
            .map_or(HistoryResolution::Ready(UndoScope::Board), |thought_id| {
                HistoryResolution::BlockedByEditor { thought_id }
            })
    }

    fn resolve_edit_operation(&self, thought_id: ThoughtId, undo: bool) -> HistoryResolution {
        let editor = self.editor_sequence(thought_id, undo);
        let board = self.board_operation(undo).filter(|operation| {
            operation.kind.belongs_to_thought_content() && operation.addresses_thought(thought_id)
        });
        if let Some(operation) =
            board.filter(|operation| board_precedes_editor(operation.sequence, editor, undo))
        {
            return self.board_dependency_blocker(operation, undo).map_or(
                HistoryResolution::Ready(UndoScope::Board),
                |blocked| HistoryResolution::BlockedByEditor {
                    thought_id: blocked,
                },
            );
        }
        if editor.is_none() {
            return HistoryResolution::Empty;
        }
        if self.editor_endpoint_matches(thought_id, undo) {
            HistoryResolution::Ready(UndoScope::Editor { thought_id })
        } else {
            HistoryResolution::BlockedByBoard { thought_id }
        }
    }

    fn board_dependency_blocker(
        &self,
        operation: &BoardOperation,
        undo: bool,
    ) -> Option<ThoughtId> {
        operation
            .content_thought_ids()
            .into_iter()
            .find(|thought_id| {
                self.editor_sequence(*thought_id, undo)
                    .is_some_and(|editor| editor_precedes_board(editor, operation.sequence, undo))
            })
    }

    fn editor_endpoint_matches(&self, thought_id: ThoughtId, undo: bool) -> bool {
        let Some(history) = self.editor_histories.get(&thought_id) else {
            return false;
        };
        let revision = if undo {
            history
                .cursor
                .checked_sub(1)
                .and_then(|index| history.revisions.get(index))
        } else {
            history.revisions.get(history.cursor)
        };
        let Some(revision) = revision else {
            return false;
        };
        let Some(current) = self
            .board
            .thought(thought_id)
            .filter(|thought| thought.is_live())
        else {
            return false;
        };
        if undo {
            current.content == revision.after_content
                && current.annotations == revision.after_annotations
        } else {
            current.content == revision.before_content
                && current.annotations == revision.before_annotations
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

fn editor_precedes_board(editor: OperationSequence, board: OperationSequence, undo: bool) -> bool {
    if undo { editor > board } else { editor < board }
}
