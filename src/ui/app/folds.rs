//! Atomic interaction with display-only folded content ranges.

use crate::{
    domain::ThoughtId,
    ports::editor::{CursorMovement, EditCommand},
    ui::annotations::PresentedSubstitution,
};
use unicode_segmentation::UnicodeSegmentation as _;

use super::BoardApp;
use crate::ui::VisualRowEdge;

impl BoardApp {
    pub(super) fn move_to_visual_row_edge(&mut self, edge: VisualRowEdge, extend_selection: bool) {
        let mut frame = self.build_frame_presentation();
        self.attach_editor_presentation(&mut frame);
        let Some(position) = frame
            .editor()
            .map(|editor| editor.visual_row_edge(edge, extend_selection))
        else {
            return;
        };
        let Some((_, editor)) = &mut self.editor else {
            return;
        };
        let _outcome = editor.apply(EditCommand::SetCursor {
            position,
            extend_selection,
        });
        self.edit_boundary = None;
    }

    pub(super) fn reveal_sentence_folds(&mut self, list_indent_width: u8) -> bool {
        let Some(thought_id) = self.active_thought_id() else {
            return false;
        };
        let Some((_, editor)) = &self.editor else {
            return false;
        };
        let ranges = editor.sentence_deletion_ranges(list_indent_width);
        if ranges.is_empty() {
            return false;
        }
        let Some(thought) = self.active_presented_thought() else {
            return false;
        };
        let targets = thought
            .presentation
            .substitutions
            .iter()
            .filter(|substitution| {
                substitution.collapsed
                    && ranges.iter().any(|range| {
                        range.start < substitution.canonical_end
                            && substitution.canonical_start < range.end
                    })
            })
            .map(|substitution| (thought_id, substitution.annotation_index))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return false;
        }
        self.expanded_folds.extend(targets);
        self.scroll_geometry = None;
        self.layout = None;
        self.set_warning("Sentence contains folded content. Review it, then delete again.");
        true
    }

    pub(super) fn expand_fold_at_cursor(&mut self) -> bool {
        let Some(thought_id) = self.active_thought_id() else {
            return false;
        };
        let Some((index, _, end)) = self.selected_collapsed_annotation() else {
            return false;
        };
        if !self.expanded_folds.insert((thought_id, index)) {
            return false;
        }
        self.set_editor_range(end, end);
        true
    }

    pub(super) fn insert_space_before_selected_fold(
        &mut self,
        ids: &mut impl crate::ports::environment::IdGenerator,
        clock: &impl crate::ports::environment::Clock,
    ) -> Option<Vec<crate::application::Effect>> {
        let thought_id = self.active_thought_id()?;
        self.selected_collapsed_annotation()?;
        let command = EditCommand::InsertBeforeSelection(' ');
        if self.edit_command_blocked(&command) {
            return Some(Vec::new());
        }
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            super::pending_types::EditFlush::Complete(effects) => effects,
            super::pending_types::EditFlush::Blocked(effects) => return Some(effects),
        };
        let expanded = self.expanded_fold_indices(thought_id);
        self.apply_edit(command);
        self.expanded_folds
            .extend(expanded.into_iter().map(|index| (thought_id, index)));
        match self.flush_edit_boundary(ids, clock) {
            super::pending_types::EditFlush::Complete(flushed)
            | super::pending_types::EditFlush::Blocked(flushed) => effects.extend(flushed),
        }
        Some(effects)
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

    pub(super) fn editor_cell_target(
        &self,
        row: u16,
        column: u16,
    ) -> Option<crate::ui::projection::BoardCellTarget> {
        self.editor_presentation()
            .map(|presentation| presentation.cell_target(row, column))
    }

    pub(super) fn normalize_fold_cursor(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
    ) {
        if extend_selection {
            return;
        }
        let Some(_) = self.active_thought_id() else {
            return;
        };
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        let cursor =
            crate::ports::text_layout::byte_for_position(&snapshot.content, snapshot.cursor);
        let target = self.active_presented_thought().and_then(|thought| {
            thought
                .presentation
                .substitutions
                .iter()
                .find(|substitution| {
                    substitution.collapsed && cursor_in_fold(cursor, substitution, movement)
                })
                .map(|substitution| (substitution.canonical_start, substitution.canonical_end))
        });
        let Some((start, end)) = target else {
            return;
        };
        self.set_editor_range(start, end);
    }

    pub(super) fn normalize_clipboard_selection(&mut self) {
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        let Some(selection) = snapshot.selection else {
            return;
        };
        let selected = (
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.start),
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.end),
        );
        let Some(normalized) = self.active_presented_thought().map(|thought| {
            thought
                .presentation
                .substitutions
                .iter()
                .filter(|substitution| {
                    substitution.collapsed
                        && selected.0 < substitution.canonical_end
                        && substitution.canonical_start < selected.1
                })
                .fold(selected, |range, substitution| {
                    (
                        range.0.min(substitution.canonical_start),
                        range.1.max(substitution.canonical_end),
                    )
                })
        }) else {
            return;
        };
        if normalized != selected {
            self.set_editor_range(normalized.0, normalized.1);
        }
    }

    pub(super) fn leave_selected_fold(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
    ) -> bool {
        if extend_selection {
            return false;
        }
        let Some(_) = self.active_thought_id() else {
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
        let target = self.active_presented_thought().and_then(|thought| {
            thought
                .presentation
                .substitutions
                .iter()
                .find(|substitution| {
                    substitution.collapsed
                        && range == (substitution.canonical_start, substitution.canonical_end)
                })
                .map(|substitution| {
                    fold_departure_target(
                        &snapshot.content,
                        substitution.canonical_start,
                        substitution.canonical_end,
                        movement,
                    )
                })
        });
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
        let Some(_) = self.active_thought_id() else {
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
        let range = self.active_presented_thought().and_then(|thought| {
            adjacent_range(cursor, backwards, &thought.presentation.substitutions)
        });
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

    fn selected_collapsed_annotation(&self) -> Option<(usize, usize, usize)> {
        let snapshot = self.editor_snapshot()?;
        let selection = snapshot.selection?;
        let selected = (
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.start),
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.end),
        );
        self.active_presented_thought()?
            .presentation
            .substitutions
            .iter()
            .find(|substitution| {
                substitution.collapsed
                    && selected == (substitution.canonical_start, substitution.canonical_end)
            })
            .map(|substitution| {
                (
                    substitution.annotation_index,
                    substitution.canonical_start,
                    substitution.canonical_end,
                )
            })
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

fn cursor_in_fold(
    cursor: usize,
    substitution: &PresentedSubstitution,
    movement: CursorMovement,
) -> bool {
    if moves_before(movement) {
        cursor > substitution.canonical_start && cursor <= substitution.canonical_end
    } else if moves_after(movement) {
        cursor >= substitution.canonical_start && cursor < substitution.canonical_end
    } else {
        cursor > substitution.canonical_start && cursor < substitution.canonical_end
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
    start: usize,
    end: usize,
    movement: CursorMovement,
) -> usize {
    match movement {
        CursorMovement::DocumentStart => 0,
        CursorMovement::DocumentEnd => content.len(),
        CursorMovement::GraphemeBack
        | CursorMovement::WordBack
        | CursorMovement::VisualUp
        | CursorMovement::VisualJumpUp
        | CursorMovement::LineStart => boundary_before_fold(content, start),
        CursorMovement::GraphemeForward
        | CursorMovement::WordForward
        | CursorMovement::VisualDown
        | CursorMovement::VisualJumpDown
        | CursorMovement::LineEnd => end,
    }
}

fn adjacent_range(
    cursor: usize,
    backwards: bool,
    substitutions: &[PresentedSubstitution],
) -> Option<(usize, usize)> {
    substitutions.iter().find_map(|substitution| {
        let boundary = if backwards {
            substitution.canonical_end == cursor
        } else {
            substitution.canonical_start == cursor
        };
        (boundary && substitution.collapsed)
            .then_some((substitution.canonical_start, substitution.canonical_end))
    })
}
