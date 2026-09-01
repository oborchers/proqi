//! Rope-backed multiline editor implementation.

mod mutation;
mod pointer;
mod smart_lists;
mod text;

use std::cmp::Ordering;

use ropey::Rope;

use crate::domain::TextPosition;
use crate::ports::editor::{
    CellRange, CursorMovement, EditCommand, EditOutcome, Editor, EditorFactory, EditorSnapshot,
    FAST_NAVIGATION_ROWS, TextChangeSet, TextSelection, TextViewport,
};
use crate::ports::text_layout::{
    WrappedRow, byte_at_cell, byte_for_position, cell_column_at_byte, logical_lines,
    position_for_byte, wrap_rows, wrapped_row_index,
};
use text::{next_boundary, previous_boundary, word_back, word_forward};

/// Factory used by outer composition to keep the UI implementation-independent.
#[derive(Clone, Copy, Debug, Default)]
pub struct RopeEditorFactory;

impl EditorFactory for RopeEditorFactory {
    fn create(&self, text: &str) -> Box<dyn Editor> {
        Box::new(RopeEditor::new(text))
    }
}

#[derive(Clone)]
struct State {
    text: Rope,
    cursor_byte: usize,
    selection_anchor_byte: Option<usize>,
}

impl State {
    fn empty() -> Self {
        Self {
            text: Rope::new(),
            cursor_byte: 0,
            selection_anchor_byte: None,
        }
    }
}

/// Rope-backed editor with grapheme-safe positions and exact content storage.
pub struct RopeEditor {
    state: State,
    undo: Vec<mutation::HistoryEntry>,
    redo: Vec<mutation::HistoryEntry>,
    viewport: TextViewport,
    scroll_row: usize,
    preferred_column: Option<usize>,
    pointer_selection: Option<pointer::PointerSelection>,
}

impl Default for RopeEditor {
    fn default() -> Self {
        Self {
            state: State::empty(),
            undo: Vec::new(),
            redo: Vec::new(),
            viewport: TextViewport::default(),
            scroll_row: 0,
            preferred_column: None,
            pointer_selection: None,
        }
    }
}

impl RopeEditor {
    /// Construct an editor containing exact text.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            state: State {
                text: Rope::from_str(text),
                cursor_byte: 0,
                selection_anchor_byte: None,
            },
            ..Self::default()
        }
    }

    fn content(&self) -> String {
        self.state.text.to_string()
    }

    fn selection_bytes(&self) -> Option<(usize, usize)> {
        let anchor = self.state.selection_anchor_byte?;
        if anchor == self.state.cursor_byte {
            return None;
        }
        Some(match anchor.cmp(&self.state.cursor_byte) {
            Ordering::Less => (anchor, self.state.cursor_byte),
            Ordering::Greater => (self.state.cursor_byte, anchor),
            Ordering::Equal => return None,
        })
    }

    fn set_cursor_byte(&mut self, byte: usize, extend_selection: bool) {
        if extend_selection {
            self.state
                .selection_anchor_byte
                .get_or_insert(self.state.cursor_byte);
        } else {
            self.state.selection_anchor_byte = None;
        }
        self.state.cursor_byte = byte.min(self.state.text.len_bytes());
        self.ensure_cursor_visible();
    }

    fn move_cursor(&mut self, movement: CursorMovement, extend_selection: bool) {
        let content = self.content();
        let target = match movement {
            CursorMovement::GraphemeBack => {
                previous_boundary(&content, self.state.cursor_byte).unwrap_or(0)
            }
            CursorMovement::GraphemeForward => {
                next_boundary(&content, self.state.cursor_byte).unwrap_or(content.len())
            }
            CursorMovement::WordBack => word_back(&content, self.state.cursor_byte),
            CursorMovement::WordForward => word_forward(&content, self.state.cursor_byte),
            CursorMovement::LineStart => {
                let lines = logical_lines(&content);
                lines[position_for_byte(&content, self.state.cursor_byte).line].start
            }
            CursorMovement::LineEnd => {
                let lines = logical_lines(&content);
                lines[position_for_byte(&content, self.state.cursor_byte).line].content_end
            }
            CursorMovement::DocumentStart => 0,
            CursorMovement::DocumentEnd => content.len(),
            CursorMovement::VisualUp => self.vertical_target(-1),
            CursorMovement::VisualDown => self.vertical_target(1),
            CursorMovement::VisualJumpUp => {
                self.vertical_target(-FAST_NAVIGATION_ROWS.cast_signed())
            }
            CursorMovement::VisualJumpDown => {
                self.vertical_target(FAST_NAVIGATION_ROWS.cast_signed())
            }
        };
        if !matches!(
            movement,
            CursorMovement::VisualUp
                | CursorMovement::VisualDown
                | CursorMovement::VisualJumpUp
                | CursorMovement::VisualJumpDown
        ) {
            self.preferred_column = None;
        }
        self.set_cursor_byte(target, extend_selection);
    }

    fn vertical_target(&mut self, rows: isize) -> usize {
        let wrapped = self.wrapped_lines();
        let current_index = wrapped_row_index(&wrapped, self.state.cursor_byte);
        let current = &wrapped[current_index];
        let current_column = cell_column_at_byte(&self.content(), current, self.state.cursor_byte);
        let preferred = *self.preferred_column.get_or_insert(current_column);
        let target_index = current_index
            .saturating_add_signed(rows)
            .min(wrapped.len().saturating_sub(1));
        byte_at_cell(&self.content(), &wrapped[target_index], preferred)
    }

    fn wrapped_lines(&self) -> Vec<WrappedRow> {
        wrap_rows(&self.content(), usize::from(self.viewport.width))
    }

    fn ensure_cursor_visible(&mut self) {
        let wrapped = self.wrapped_lines();
        let cursor_row = wrapped_row_index(&wrapped, self.state.cursor_byte);
        let height = usize::from(self.viewport.height);
        if cursor_row < self.scroll_row {
            self.scroll_row = cursor_row;
        } else if cursor_row >= self.scroll_row + height {
            self.scroll_row = cursor_row + 1 - height;
        }
        self.scroll_row = self
            .scroll_row
            .min(wrapped.len().saturating_sub(height.max(1)));
    }

    fn selection(&self, content: &str) -> Option<TextSelection> {
        let (start, end) = self.selection_bytes()?;
        Some(TextSelection {
            start: position_for_byte(content, start),
            end: position_for_byte(content, end),
        })
    }
}

impl Editor for RopeEditor {
    fn apply(&mut self, command: EditCommand) -> EditOutcome {
        let changes = match command {
            EditCommand::InsertChar(character) => {
                self.mutate(|editor| editor.replace_selection_or_insert(&character.to_string()))
            }
            EditCommand::InsertBeforeSelection(c) => self.insert_before_selection(c),
            EditCommand::Paste(text) => {
                self.mutate(|editor| editor.replace_selection_or_insert(&text))
            }
            EditCommand::InsertNewline => {
                self.mutate(|editor| editor.replace_selection_or_insert("\n"))
            }
            EditCommand::InsertSmartNewline { indent_width } => {
                self.mutate(|editor| editor.insert_smart_newline(indent_width))
            }
            EditCommand::Indent { width, smart_lists } => {
                self.mutate_many(|editor| editor.indent(width, smart_lists))
            }
            EditCommand::Outdent { width, smart_lists } => {
                self.mutate_many(|editor| editor.outdent(width, smart_lists))
            }
            EditCommand::DeleteBack => self.mutate(Self::delete_back),
            EditCommand::DeleteForward => self.mutate(Self::delete_forward),
            EditCommand::DeleteLogicalLine => self.mutate(Self::delete_logical_line),
            EditCommand::Move {
                movement,
                extend_selection,
            } => {
                self.pointer_selection = None;
                self.move_cursor(movement, extend_selection);
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::SelectAll => {
                self.pointer_selection = None;
                self.preferred_column = None;
                self.state.selection_anchor_byte = Some(0);
                self.state.cursor_byte = self.state.text.len_bytes();
                self.ensure_cursor_visible();
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::ClearSelection => {
                self.pointer_selection = None;
                self.preferred_column = None;
                self.state.selection_anchor_byte = None;
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::SetCursor {
                position,
                extend_selection,
            } => {
                self.pointer_selection = None;
                self.preferred_column = None;
                let byte = byte_for_position(&self.content(), position);
                self.set_cursor_byte(byte, extend_selection);
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::PointerStart {
                position,
                granularity,
                extend_selection,
            } => {
                self.preferred_column = None;
                self.begin_pointer_selection(position, granularity, extend_selection);
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::PointerDrag { position } => {
                self.extend_pointer_selection(position);
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::PointerEnd => {
                self.end_pointer_selection();
                TextChangeSet::unchanged(self.state.text.len_bytes())
            }
            EditCommand::Undo => self.undo_edit(),
            EditCommand::Redo => self.redo_edit(),
        };

        EditOutcome {
            changes,
            snapshot: self.snapshot(),
        }
    }

    fn set_viewport(&mut self, viewport: TextViewport) {
        self.viewport = TextViewport::new(viewport.width, viewport.height);
        self.ensure_cursor_visible();
    }

    fn scroll_by(&mut self, rows: isize) {
        let maximum = self
            .wrapped_lines()
            .len()
            .saturating_sub(usize::from(self.viewport.height).max(1));
        self.scroll_row = self.scroll_row.saturating_add_signed(rows).min(maximum);
    }

    fn snapshot(&self) -> EditorSnapshot {
        let content = self.content();
        let selected_bytes = self.selection_bytes();
        EditorSnapshot {
            cursor: position_for_byte(&content, self.state.cursor_byte),
            selection: self.selection(&content),
            viewport: self.viewport,
            scroll_row: self.scroll_row,
            visual_lines: self
                .wrapped_lines()
                .into_iter()
                .map(|mut line| {
                    line.visual.selected_cells = selected_bytes.and_then(|(start, end)| {
                        let selected_start = start.max(line.start_byte);
                        let selected_end = end.min(line.end_byte);
                        (selected_start < selected_end).then(|| CellRange {
                            start: cell_column_at_byte(&content, &line, selected_start),
                            end: cell_column_at_byte(&content, &line, selected_end),
                        })
                    });
                    line.visual
                })
                .collect(),
            content,
        }
    }

    fn replace_content(&mut self, text: String, cursor: TextPosition) -> EditOutcome {
        let before = self.content();
        let changes = TextChangeSet::replace_all(&before, &text);
        let byte = byte_for_position(&text, cursor);
        self.state = State {
            text: Rope::from_str(&text),
            cursor_byte: byte,
            selection_anchor_byte: None,
        };
        self.undo.clear();
        self.redo.clear();
        self.preferred_column = None;
        self.pointer_selection = None;
        self.ensure_cursor_visible();
        EditOutcome {
            changes,
            snapshot: self.snapshot(),
        }
    }

    fn position_at_cell(&self, row: u16, column: u16) -> TextPosition {
        let content = self.content();
        let lines = self.wrapped_lines();
        let index = (self.scroll_row + usize::from(row)).min(lines.len().saturating_sub(1));
        let byte = byte_at_cell(&content, &lines[index], usize::from(column));
        position_for_byte(&content, byte)
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_bytes()?;
        let content = self.content();
        Some(content[start..end].to_owned())
    }
}
