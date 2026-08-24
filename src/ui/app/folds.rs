//! Atomic interaction with display-only folded content ranges.

use crate::{
    domain::{ContentAnnotation, ThoughtId},
    ports::editor::{CursorMovement, EditCommand},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn expand_fold_at_cursor(&mut self) -> bool {
        let Some(thought_id) = self.active_thought_id() else {
            return false;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return false;
        };
        let cursor = crate::ui::projection::byte_for_position(&snapshot.content, snapshot.cursor);
        let annotations = self.current_annotations(thought_id);
        let candidate = annotations
            .iter()
            .enumerate()
            .find_map(|(index, annotation)| {
                (!self.expanded_folds.contains(&(thought_id, index))
                    && (cursor == annotation.start || cursor == annotation.end))
                    .then_some(index)
            });
        candidate.is_some_and(|index| self.expanded_folds.insert((thought_id, index)))
    }

    pub(super) fn expand_fold_at_cell(
        &mut self,
        thought_id: ThoughtId,
        row: u16,
        column: u16,
    ) -> bool {
        if self.active_thought_id() != Some(thought_id) {
            return false;
        }
        let Some(presentation) = self.editor_presentation() else {
            return false;
        };
        let Some(fold) = presentation.fold_at_cell(row, column) else {
            return false;
        };
        let index = fold.annotation_index;
        let end = fold.canonical_end;
        if !self.expanded_folds.insert((thought_id, index)) {
            return false;
        }
        if let Some((_, editor)) = &mut self.editor {
            let content = editor.snapshot().content;
            let position = crate::ui::projection::position_for_byte(&content, end);
            let _outcome = editor.apply(EditCommand::SetCursor {
                position,
                extend_selection: false,
            });
        }
        true
    }

    pub(super) fn projected_position_at_cell(
        &self,
        row: u16,
        column: u16,
    ) -> crate::domain::TextPosition {
        self.editor_presentation()
            .map_or_else(crate::domain::TextPosition::default, |presentation| {
                presentation.canonical_position_at_cell(row, column)
            })
    }

    pub(super) fn normalize_fold_cursor(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
    ) {
        let Some(thought_id) = self.active_thought_id() else {
            return;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        let cursor = crate::ui::projection::byte_for_position(&snapshot.content, snapshot.cursor);
        let annotations = self.current_annotations(thought_id);
        let target = annotations
            .iter()
            .enumerate()
            .find_map(|(index, annotation)| {
                (!self.expanded_folds.contains(&(thought_id, index))
                    && cursor > annotation.start
                    && cursor < annotation.end)
                    .then(|| movement_target(annotation, movement))
            });
        let Some(target) = target else {
            return;
        };
        if let Some((_, editor)) = &mut self.editor {
            let position = crate::ui::projection::position_for_byte(&snapshot.content, target);
            let _outcome = editor.apply(EditCommand::SetCursor {
                position,
                extend_selection,
            });
        }
    }

    pub(super) fn delete_adjacent_fold(&mut self, backwards: bool) -> bool {
        let Some(thought_id) = self.active_thought_id() else {
            return false;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return false;
        };
        if snapshot.selection.is_some() {
            return false;
        }
        let cursor = crate::ui::projection::byte_for_position(&snapshot.content, snapshot.cursor);
        let annotations = self.current_annotations(thought_id);
        let range = adjacent_range(
            thought_id,
            cursor,
            backwards,
            &annotations,
            &self.expanded_folds,
        );
        let Some((start, end)) = range else {
            return false;
        };
        let Some((_, editor)) = &mut self.editor else {
            return false;
        };
        let end = crate::ui::projection::position_for_byte(&snapshot.content, end);
        let start = crate::ui::projection::position_for_byte(&snapshot.content, start);
        let _outcome = editor.apply(EditCommand::SetCursor {
            position: end,
            extend_selection: false,
        });
        let _outcome = editor.apply(EditCommand::SetCursor {
            position: start,
            extend_selection: true,
        });
        true
    }

    pub(super) fn clear_expanded_folds(&mut self, thought_id: ThoughtId) {
        self.expanded_folds.retain(|(id, _)| *id != thought_id);
    }
}

fn movement_target(annotation: &ContentAnnotation, movement: CursorMovement) -> usize {
    match movement {
        CursorMovement::GraphemeBack
        | CursorMovement::WordBack
        | CursorMovement::VisualUp
        | CursorMovement::LineStart
        | CursorMovement::DocumentStart => annotation.start,
        CursorMovement::GraphemeForward
        | CursorMovement::WordForward
        | CursorMovement::VisualDown
        | CursorMovement::LineEnd
        | CursorMovement::DocumentEnd => annotation.end,
    }
}

fn adjacent_range(
    thought_id: ThoughtId,
    cursor: usize,
    backwards: bool,
    annotations: &[ContentAnnotation],
    expanded: &std::collections::BTreeSet<(ThoughtId, usize)>,
) -> Option<(usize, usize)> {
    annotations
        .iter()
        .enumerate()
        .find_map(|(index, annotation)| {
            let boundary = if backwards {
                annotation.end == cursor
            } else {
                annotation.start == cursor
            };
            (boundary && !expanded.contains(&(thought_id, index)))
                .then_some((annotation.start, annotation.end))
        })
}
