//! Canonical-to-visible mapping for folded editor ranges.

use super::annotations::{Presentation, PresentedFold, project};
use crate::{
    domain::{ContentAnnotation, TextPosition},
    ports::{
        editor::{CellRange, EditorSnapshot, TextSelection},
        text_layout::{
            WrappedRow, byte_at_cell, byte_for_position, cell_column_at_byte, position_for_byte,
            wrap_rows, wrapped_row_index,
        },
    },
};

/// Fold-aware visible editor state with lossless canonical mappings.
pub(super) struct EditorPresentation {
    pub(super) snapshot: EditorSnapshot,
    pub(super) folds: Vec<PresentedFold>,
    canonical_content: String,
    rows: Vec<WrappedRow>,
    cursor_display_byte: usize,
}

impl EditorPresentation {
    pub(super) fn fold_at_cell(&self, row: u16, column: u16) -> Option<&PresentedFold> {
        let row = self
            .rows
            .get(self.snapshot.scroll_row.saturating_add(usize::from(row)))?;
        let byte = byte_at_cell(&self.snapshot.content, row, usize::from(column));
        self.folds.iter().find(|fold| {
            fold.collapsed
                && byte >= fold.start
                && (byte < fold.end || (byte == fold.end && fold.start == row.end_byte))
        })
    }

    pub(super) fn canonical_position_at_cell(&self, row: u16, column: u16) -> TextPosition {
        let Some(row) = self
            .rows
            .get(self.snapshot.scroll_row.saturating_add(usize::from(row)))
        else {
            return TextPosition::default();
        };
        let display = byte_at_cell(&self.snapshot.content, row, usize::from(column));
        position_for_byte(
            &self.canonical_content,
            unproject_byte(display, &self.folds),
        )
    }

    pub(super) fn cursor_viewport_cell(&self) -> Option<(usize, usize)> {
        let cursor = self.cursor_display_byte;
        let row = wrapped_row_index(&self.rows, cursor);
        let viewport_row = row.checked_sub(self.snapshot.scroll_row)?;
        let wrapped = self.rows.get(row)?;
        Some((
            cell_column_at_byte(&self.snapshot.content, wrapped, cursor),
            viewport_row,
        ))
    }
}

pub(super) fn editor_presentation(
    canonical: &EditorSnapshot,
    annotations: &[ContentAnnotation],
    expanded: &[usize],
) -> EditorPresentation {
    let presentation = project(&canonical.content, annotations, expanded);
    let cursor_display_byte = project_byte(
        byte_for_position(&canonical.content, canonical.cursor),
        &presentation.folds,
    );
    let cursor = position_for_byte(&presentation.content, cursor_display_byte);
    let selection = canonical.selection.map(|selection| TextSelection {
        start: project_position(&canonical.content, selection.start, &presentation),
        end: project_position(&canonical.content, selection.end, &presentation),
    });
    let mut rows = wrap_rows(
        &presentation.content,
        usize::from(canonical.viewport.width.max(1)),
    );
    apply_selection(&presentation.content, &mut rows, selection);
    let scroll_row = visible_scroll(canonical, &rows, cursor_display_byte);
    let snapshot = EditorSnapshot {
        content: presentation.content,
        cursor,
        selection,
        viewport: canonical.viewport,
        scroll_row,
        visual_lines: rows.iter().map(|row| row.visual.clone()).collect(),
    };
    EditorPresentation {
        snapshot,
        folds: presentation.folds,
        canonical_content: canonical.content.clone(),
        rows,
        cursor_display_byte,
    }
}

fn project_position(
    canonical: &str,
    position: TextPosition,
    presentation: &Presentation,
) -> TextPosition {
    let byte = byte_for_position(canonical, position);
    let projected = project_byte(byte, &presentation.folds);
    position_for_byte(&presentation.content, projected)
}

fn project_byte(byte: usize, folds: &[PresentedFold]) -> usize {
    let mut canonical_cursor = 0;
    let mut display_cursor = 0;
    for fold in folds {
        if byte < fold.canonical_start {
            return display_cursor + byte.saturating_sub(canonical_cursor);
        }
        if byte <= fold.canonical_end {
            if !fold.collapsed {
                return fold.start + byte.saturating_sub(fold.canonical_start);
            }
            return if byte == fold.canonical_start {
                fold.start
            } else {
                fold.end
            };
        }
        canonical_cursor = fold.canonical_end;
        display_cursor = fold.end;
    }
    display_cursor + byte.saturating_sub(canonical_cursor)
}

fn unproject_byte(byte: usize, folds: &[PresentedFold]) -> usize {
    let mut canonical_cursor = 0;
    let mut display_cursor = 0;
    for fold in folds {
        if byte < fold.start {
            return canonical_cursor + byte.saturating_sub(display_cursor);
        }
        if byte <= fold.end {
            return if fold.collapsed {
                fold.canonical_end
            } else {
                fold.canonical_start + byte.saturating_sub(fold.start)
            };
        }
        canonical_cursor = fold.canonical_end;
        display_cursor = fold.end;
    }
    canonical_cursor + byte.saturating_sub(display_cursor)
}

fn apply_selection(content: &str, rows: &mut [WrappedRow], selection: Option<TextSelection>) {
    let Some(selection) = selection else {
        return;
    };
    let start = byte_for_position(content, selection.start);
    let end = byte_for_position(content, selection.end);
    for row in rows {
        let selected_start = start.max(row.start_byte);
        let selected_end = end.min(row.end_byte);
        row.visual.selected_cells = (selected_start < selected_end).then(|| CellRange {
            start: cell_column_at_byte(content, row, selected_start),
            end: cell_column_at_byte(content, row, selected_end),
        });
    }
}

fn visible_scroll(canonical: &EditorSnapshot, rows: &[WrappedRow], cursor: usize) -> usize {
    let cursor_row = wrapped_row_index(rows, cursor);
    let height = usize::from(canonical.viewport.height).max(1);
    let mut scroll = canonical.scroll_row.min(rows.len().saturating_sub(height));
    if cursor_row < scroll {
        scroll = cursor_row;
    } else if cursor_row >= scroll + height {
        scroll = cursor_row + 1 - height;
    }
    scroll.min(rows.len().saturating_sub(height))
}
