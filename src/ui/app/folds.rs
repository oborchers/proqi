//! Atomic interaction with display-only folded content ranges.

use crate::{
    domain::{AnnotationBehavior, ContentAnnotation, ThoughtId},
    ports::editor::{CursorMovement, EditCommand},
};
use unicode_segmentation::UnicodeSegmentation as _;

use super::BoardApp;

impl BoardApp {
    pub(super) fn expand_fold_at_cursor(&mut self) -> bool {
        let Some(thought_id) = self.active_thought_id() else {
            return false;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return false;
        };
        let selection = snapshot.selection.map(|selection| {
            (
                crate::ports::text_layout::byte_for_position(&snapshot.content, selection.start),
                crate::ports::text_layout::byte_for_position(&snapshot.content, selection.end),
            )
        });
        let annotations = self.current_annotations(thought_id);
        let candidate = annotations
            .iter()
            .enumerate()
            .find_map(|(index, annotation)| {
                (!self.expanded_folds.contains(&(thought_id, index))
                    && annotation.kind.behavior() == AnnotationBehavior::Substitution
                    && selection == Some((annotation.start, annotation.end)))
                .then_some(index)
            });
        let Some(index) = candidate else {
            return false;
        };
        if !self.expanded_folds.insert((thought_id, index)) {
            return false;
        }
        let end = annotations[index].end;
        self.set_editor_range(end, end);
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

    pub(super) fn select_fold_at_cell(
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
        self.set_editor_range(fold.canonical_start, fold.canonical_end);
        true
    }

    pub(super) fn normalize_fold_cursor(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
    ) {
        if extend_selection {
            return;
        }
        let Some(thought_id) = self.active_thought_id() else {
            return;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        let cursor =
            crate::ports::text_layout::byte_for_position(&snapshot.content, snapshot.cursor);
        let annotations = self.current_annotations(thought_id);
        let target = annotations
            .iter()
            .enumerate()
            .find_map(|(index, annotation)| {
                (!self.expanded_folds.contains(&(thought_id, index))
                    && annotation.kind.behavior() == AnnotationBehavior::Substitution
                    && cursor_in_fold(cursor, annotation, movement))
                .then_some((annotation.start, annotation.end))
            });
        let Some((start, end)) = target else {
            return;
        };
        self.set_editor_range(start, end);
    }

    pub(super) fn leave_selected_fold(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
    ) -> bool {
        if extend_selection {
            return false;
        }
        let Some(thought_id) = self.active_thought_id() else {
            return false;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return false;
        };
        let Some(selection) = snapshot.selection else {
            return false;
        };
        let range = (
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.start),
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.end),
        );
        let annotations = self.current_annotations(thought_id);
        let target = annotations
            .iter()
            .enumerate()
            .find(|(index, annotation)| {
                !self.expanded_folds.contains(&(thought_id, *index))
                    && annotation.kind.behavior() == AnnotationBehavior::Substitution
                    && range == (annotation.start, annotation.end)
            })
            .map(|(_, annotation)| fold_departure_target(&snapshot.content, annotation, movement));
        let Some(target) = target else {
            return false;
        };
        self.set_editor_range(target, target);
        !matches!(
            movement,
            CursorMovement::VisualJumpUp | CursorMovement::VisualJumpDown
        )
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
        let cursor =
            crate::ports::text_layout::byte_for_position(&snapshot.content, snapshot.cursor);
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
        let end = crate::ports::text_layout::position_for_byte(&snapshot.content, end);
        let start = crate::ports::text_layout::position_for_byte(&snapshot.content, start);
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

    pub(super) fn set_editor_range(&mut self, start: usize, end: usize) {
        let Some((_, editor)) = &mut self.editor else {
            return;
        };
        let content = editor.snapshot().content;
        let start = crate::ports::text_layout::position_for_byte(&content, start);
        let end = crate::ports::text_layout::position_for_byte(&content, end);
        let _outcome = editor.apply(EditCommand::SetCursor {
            position: start,
            extend_selection: false,
        });
        let _outcome = editor.apply(EditCommand::SetCursor {
            position: end,
            extend_selection: true,
        });
    }

    pub(super) fn clear_expanded_folds(&mut self, thought_id: ThoughtId) {
        self.expanded_folds.retain(|(id, _)| *id != thought_id);
    }
}

fn boundary_before_fold(content: &str, fold_start: usize) -> usize {
    content
        .get(..fold_start)
        .and_then(|prefix| prefix.grapheme_indices(true).next_back())
        .filter(|(_, grapheme)| *grapheme != "\n" && grapheme.chars().all(char::is_whitespace))
        .map_or(fold_start, |(byte, _)| byte)
}

fn moves_before(movement: CursorMovement) -> bool {
    matches!(
        movement,
        CursorMovement::GraphemeBack
            | CursorMovement::WordBack
            | CursorMovement::VisualUp
            | CursorMovement::VisualJumpUp
            | CursorMovement::LineStart
            | CursorMovement::DocumentStart
    )
}

fn cursor_in_fold(cursor: usize, annotation: &ContentAnnotation, movement: CursorMovement) -> bool {
    if moves_before(movement) {
        cursor > annotation.start && cursor <= annotation.end
    } else if moves_after(movement) {
        cursor >= annotation.start && cursor < annotation.end
    } else {
        cursor > annotation.start && cursor < annotation.end
    }
}

fn moves_after(movement: CursorMovement) -> bool {
    matches!(
        movement,
        CursorMovement::GraphemeForward
            | CursorMovement::WordForward
            | CursorMovement::VisualDown
            | CursorMovement::VisualJumpDown
            | CursorMovement::LineEnd
            | CursorMovement::DocumentEnd
    )
}

fn fold_departure_target(
    content: &str,
    annotation: &ContentAnnotation,
    movement: CursorMovement,
) -> usize {
    match movement {
        CursorMovement::DocumentStart => 0,
        CursorMovement::DocumentEnd => content.len(),
        CursorMovement::GraphemeBack
        | CursorMovement::WordBack
        | CursorMovement::VisualUp
        | CursorMovement::VisualJumpUp
        | CursorMovement::LineStart => boundary_before_fold(content, annotation.start),
        CursorMovement::GraphemeForward
        | CursorMovement::WordForward
        | CursorMovement::VisualDown
        | CursorMovement::VisualJumpDown
        | CursorMovement::LineEnd => annotation.end,
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
            (boundary
                && annotation.kind.behavior() == AnnotationBehavior::Substitution
                && !expanded.contains(&(thought_id, index)))
            .then_some((annotation.start, annotation.end))
        })
}
