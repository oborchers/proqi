//! Rope-backed multiline editor implementation.

use std::cmp::Ordering;

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::ports::editor::{
    CursorMovement, EditCommand, EditOutcome, Editor, EditorSnapshot, TextPosition, TextSelection,
    TextViewport, VisualLine,
};

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

#[derive(Clone, Copy, Debug)]
struct LogicalLine {
    start: usize,
    content_end: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct WrappedLine {
    public: VisualLine,
    start_byte: usize,
    end_byte: usize,
}

/// Rope-backed editor with grapheme-safe positions and exact content storage.
pub struct RopeEditor {
    state: State,
    undo: Vec<State>,
    redo: Vec<State>,
    viewport: TextViewport,
    scroll_row: usize,
    preferred_column: Option<usize>,
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

    fn mutate(&mut self, operation: impl FnOnce(&mut Self) -> bool) -> bool {
        let before = self.state.clone();
        if operation(self) {
            self.undo.push(before);
            self.redo.clear();
            self.preferred_column = None;
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    fn replace_byte_range(&mut self, start: usize, end: usize, replacement: &str) {
        let start_char = self.state.text.byte_to_char(start);
        let end_char = self.state.text.byte_to_char(end);
        self.state.text.remove(start_char..end_char);
        self.state.text.insert(start_char, replacement);
        self.state.cursor_byte = start + replacement.len();
        self.state.selection_anchor_byte = None;
    }

    fn selection_bytes(&self) -> Option<(usize, usize)> {
        let anchor = self.state.selection_anchor_byte?;
        if anchor == self.state.cursor_byte {
            return None;
        }
        Some(match anchor.cmp(&self.state.cursor_byte) {
            Ordering::Less => (anchor, self.state.cursor_byte),
            Ordering::Greater => (self.state.cursor_byte, anchor),
            Ordering::Equal => unreachable!("equal selection endpoints were handled above"),
        })
    }

    fn replace_selection_or_insert(&mut self, text: &str) -> bool {
        if text.is_empty() && self.selection_bytes().is_none() {
            return false;
        }
        let (start, end) = self
            .selection_bytes()
            .unwrap_or((self.state.cursor_byte, self.state.cursor_byte));
        self.replace_byte_range(start, end, text);
        true
    }

    fn delete_back(&mut self) -> bool {
        if let Some((start, end)) = self.selection_bytes() {
            self.replace_byte_range(start, end, "");
            return start != end;
        }
        let content = self.content();
        let Some(previous) = previous_boundary(&content, self.state.cursor_byte) else {
            return false;
        };
        self.replace_byte_range(previous, self.state.cursor_byte, "");
        true
    }

    fn delete_forward(&mut self) -> bool {
        if let Some((start, end)) = self.selection_bytes() {
            self.replace_byte_range(start, end, "");
            return start != end;
        }
        let content = self.content();
        let Some(next) = next_boundary(&content, self.state.cursor_byte) else {
            return false;
        };
        self.replace_byte_range(self.state.cursor_byte, next, "");
        true
    }

    fn delete_logical_line(&mut self) -> bool {
        let content = self.content();
        let lines = logical_lines(&content);
        let position = position_for_byte(&content, self.state.cursor_byte);
        let line_index = position.line.min(lines.len().saturating_sub(1));
        let line = lines[line_index];

        let (start, end) = if lines.len() == 1 {
            (0, content.len())
        } else if line_index + 1 < lines.len() {
            (line.start, line.end)
        } else {
            (lines[line_index - 1].content_end, line.end)
        };
        if start == end {
            return false;
        }
        self.replace_byte_range(start, end, "");
        true
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
        };
        if !matches!(
            movement,
            CursorMovement::VisualUp | CursorMovement::VisualDown
        ) {
            self.preferred_column = None;
        }
        self.set_cursor_byte(target, extend_selection);
    }

    fn vertical_target(&mut self, direction: i8) -> usize {
        let wrapped = self.wrapped_lines();
        let current_index = wrapped_line_index(&wrapped, self.state.cursor_byte);
        let current = &wrapped[current_index];
        let current_column = cell_column_at_byte(&self.content(), current, self.state.cursor_byte);
        let preferred = *self.preferred_column.get_or_insert(current_column);
        let target_index = if direction < 0 {
            current_index.saturating_sub(1)
        } else {
            (current_index + 1).min(wrapped.len().saturating_sub(1))
        };
        byte_at_cell(&self.content(), &wrapped[target_index], preferred)
    }

    fn wrapped_lines(&self) -> Vec<WrappedLine> {
        wrap_content(&self.content(), usize::from(self.viewport.width))
    }

    fn ensure_cursor_visible(&mut self) {
        let wrapped = self.wrapped_lines();
        let cursor_row = wrapped_line_index(&wrapped, self.state.cursor_byte);
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
        let content_changed = match command {
            EditCommand::InsertChar(character) => {
                self.mutate(|editor| editor.replace_selection_or_insert(&character.to_string()))
            }
            EditCommand::Paste(text) => {
                self.mutate(|editor| editor.replace_selection_or_insert(&text))
            }
            EditCommand::InsertNewline => {
                self.mutate(|editor| editor.replace_selection_or_insert("\n"))
            }
            EditCommand::DeleteBack => self.mutate(Self::delete_back),
            EditCommand::DeleteForward => self.mutate(Self::delete_forward),
            EditCommand::DeleteLogicalLine => self.mutate(Self::delete_logical_line),
            EditCommand::Move {
                movement,
                extend_selection,
            } => {
                self.move_cursor(movement, extend_selection);
                false
            }
            EditCommand::SelectAll => {
                self.state.selection_anchor_byte = Some(0);
                self.state.cursor_byte = self.state.text.len_bytes();
                self.ensure_cursor_visible();
                false
            }
            EditCommand::ClearSelection => {
                self.state.selection_anchor_byte = None;
                false
            }
            EditCommand::SetCursor {
                position,
                extend_selection,
            } => {
                let byte = byte_for_position(&self.content(), position);
                self.set_cursor_byte(byte, extend_selection);
                false
            }
            EditCommand::PointerStart { row, column } => {
                let position = self.position_at_cell(row, column);
                let byte = byte_for_position(&self.content(), position);
                self.set_cursor_byte(byte, false);
                self.state.selection_anchor_byte = Some(byte);
                false
            }
            EditCommand::PointerDrag { row, column } => {
                let position = self.position_at_cell(row, column);
                let byte = byte_for_position(&self.content(), position);
                self.set_cursor_byte(byte, true);
                false
            }
            EditCommand::Undo => {
                if let Some(previous) = self.undo.pop() {
                    self.redo.push(self.state.clone());
                    self.state = previous;
                    self.preferred_column = None;
                    self.ensure_cursor_visible();
                    true
                } else {
                    false
                }
            }
            EditCommand::Redo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(self.state.clone());
                    self.state = next;
                    self.preferred_column = None;
                    self.ensure_cursor_visible();
                    true
                } else {
                    false
                }
            }
        };

        EditOutcome {
            content_changed,
            snapshot: self.snapshot(),
        }
    }

    fn set_viewport(&mut self, viewport: TextViewport) {
        self.viewport = TextViewport::new(viewport.width, viewport.height);
        self.ensure_cursor_visible();
    }

    fn snapshot(&self) -> EditorSnapshot {
        let content = self.content();
        EditorSnapshot {
            cursor: position_for_byte(&content, self.state.cursor_byte),
            selection: self.selection(&content),
            viewport: self.viewport,
            scroll_row: self.scroll_row,
            visual_lines: self
                .wrapped_lines()
                .into_iter()
                .map(|line| line.public)
                .collect(),
            content,
        }
    }

    fn replace_content(&mut self, text: String, cursor: TextPosition) {
        let byte = byte_for_position(&text, cursor);
        self.state = State {
            text: Rope::from_str(&text),
            cursor_byte: byte,
            selection_anchor_byte: None,
        };
        self.undo.clear();
        self.redo.clear();
        self.preferred_column = None;
        self.ensure_cursor_visible();
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

fn logical_lines(content: &str) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (newline, _) in content.match_indices('\n') {
        let content_end = if newline > start && content.as_bytes()[newline - 1] == b'\r' {
            newline - 1
        } else {
            newline
        };
        lines.push(LogicalLine {
            start,
            content_end,
            end: newline + 1,
        });
        start = newline + 1;
    }
    lines.push(LogicalLine {
        start,
        content_end: content.len(),
        end: content.len(),
    });
    lines
}

fn byte_for_position(content: &str, position: TextPosition) -> usize {
    let lines = logical_lines(content);
    let line = lines[position.line.min(lines.len().saturating_sub(1))];
    let text = &content[line.start..line.content_end];
    text.grapheme_indices(true)
        .nth(position.grapheme)
        .map_or(line.content_end, |(offset, _)| line.start + offset)
}

fn position_for_byte(content: &str, byte: usize) -> TextPosition {
    let byte = byte.min(content.len());
    let lines = logical_lines(content);
    let line_index = lines
        .iter()
        .enumerate()
        .position(|(index, line)| byte < line.end || index + 1 == lines.len())
        .unwrap_or_else(|| lines.len().saturating_sub(1));
    let line = lines[line_index];
    let local_end = byte.min(line.content_end).saturating_sub(line.start);
    let grapheme = content[line.start..line.start + local_end]
        .graphemes(true)
        .count();
    TextPosition::new(line_index, grapheme)
}

fn previous_boundary(content: &str, cursor: usize) -> Option<usize> {
    if cursor == 0 {
        return None;
    }
    let position = position_for_byte(content, cursor);
    let lines = logical_lines(content);
    let line = lines[position.line];
    if cursor > line.start {
        return content[line.start..cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(offset, _)| line.start + offset);
    }
    (position.line > 0).then_some(lines[position.line - 1].content_end)
}

fn next_boundary(content: &str, cursor: usize) -> Option<usize> {
    if cursor >= content.len() {
        return None;
    }
    let position = position_for_byte(content, cursor);
    let lines = logical_lines(content);
    let line = lines[position.line];
    if cursor < line.content_end {
        let grapheme = content[cursor..line.content_end].graphemes(true).next()?;
        return Some(cursor + grapheme.len());
    }
    (position.line + 1 < lines.len()).then_some(lines[position.line + 1].start)
}

fn word_segments(content: &str) -> Vec<(usize, usize)> {
    content
        .split_word_bound_indices()
        .filter_map(|(start, segment)| {
            segment
                .unicode_words()
                .next()
                .map(|_| (start, start + segment.len()))
        })
        .collect()
}

fn word_back(content: &str, cursor: usize) -> usize {
    word_segments(content)
        .into_iter()
        .rev()
        .find_map(|(start, _)| (start < cursor).then_some(start))
        .unwrap_or(0)
}

fn word_forward(content: &str, cursor: usize) -> usize {
    word_segments(content)
        .into_iter()
        .find_map(|(start, end)| {
            if cursor < end {
                Some(if cursor >= start { end } else { start })
            } else {
                None
            }
        })
        .unwrap_or(content.len())
}

fn wrap_content(content: &str, width: usize) -> Vec<WrappedLine> {
    let width = width.max(1);
    let mut output = Vec::new();
    for (logical_line, line) in logical_lines(content).into_iter().enumerate() {
        let text = &content[line.start..line.content_end];
        let graphemes: Vec<_> = text.grapheme_indices(true).collect();
        if graphemes.is_empty() {
            output.push(WrappedLine {
                public: VisualLine {
                    logical_line,
                    start_grapheme: 0,
                    end_grapheme: 0,
                    cell_width: 0,
                    text: String::new(),
                },
                start_byte: line.start,
                end_byte: line.content_end,
            });
            continue;
        }

        let mut start_index = 0;
        while start_index < graphemes.len() {
            let mut end_index = start_index;
            let mut cells = 0;
            while end_index < graphemes.len() {
                let grapheme_width = UnicodeWidthStr::width(graphemes[end_index].1);
                if end_index > start_index && cells + grapheme_width > width {
                    break;
                }
                cells += grapheme_width;
                end_index += 1;
                if cells >= width {
                    break;
                }
            }
            let start_offset = graphemes[start_index].0;
            let end_offset = graphemes
                .get(end_index)
                .map_or(text.len(), |(offset, _)| *offset);
            output.push(WrappedLine {
                public: VisualLine {
                    logical_line,
                    start_grapheme: start_index,
                    end_grapheme: end_index,
                    cell_width: cells,
                    text: text[start_offset..end_offset].to_owned(),
                },
                start_byte: line.start + start_offset,
                end_byte: line.start + end_offset,
            });
            start_index = end_index;
        }
    }
    output
}

fn byte_at_cell(content: &str, line: &WrappedLine, target_cell: usize) -> usize {
    let text = &content[line.start_byte..line.end_byte];
    let mut cells = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if target_cell < cells + width {
            return line.start_byte + offset;
        }
        cells += width;
    }
    line.end_byte
}

fn cell_column_at_byte(content: &str, line: &WrappedLine, byte: usize) -> usize {
    let end = byte.min(line.end_byte);
    UnicodeWidthStr::width(&content[line.start_byte..end])
}

fn wrapped_line_index(lines: &[WrappedLine], byte: usize) -> usize {
    lines
        .iter()
        .position(|line| {
            (line.start_byte == line.end_byte && byte == line.start_byte)
                || (byte >= line.start_byte && byte < line.end_byte)
        })
        .or_else(|| lines.iter().rposition(|line| line.start_byte <= byte))
        .unwrap_or(0)
}
