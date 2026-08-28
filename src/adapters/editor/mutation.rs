//! Exact Rope mutations and transient undo records.

use std::ops::Range;

use crate::ports::editor::{TextChange, TextChangeSet};
use crate::ports::text_layout::{logical_lines, position_for_byte};

use super::{RopeEditor, State, text};

#[derive(Clone)]
pub(super) struct HistoryEntry {
    before: State,
    after: State,
    changes: TextChangeSet,
}

pub(super) struct AppliedRange {
    old: Range<usize>,
    new: Range<usize>,
}

impl RopeEditor {
    pub(super) fn mutate(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Option<AppliedRange>,
    ) -> TextChangeSet {
        let before_state = self.state.clone();
        let before_content = self.content();
        let Some(applied) = operation(self) else {
            return TextChangeSet::unchanged(before_content.len());
        };
        self.preferred_column = None;
        self.pointer_selection = None;
        self.ensure_cursor_visible();
        let after_content = self.content();
        if before_content == after_content {
            return TextChangeSet::unchanged(before_content.len());
        }
        let changes = TextChange::new(&before_content, &after_content, applied.old, applied.new)
            .and_then(|change| TextChangeSet::new(&before_content, &after_content, vec![change]));
        let Ok(changes) = changes else {
            self.state = before_state;
            self.ensure_cursor_visible();
            return TextChangeSet::unchanged(before_content.len());
        };
        self.undo.push(HistoryEntry {
            before: before_state,
            after: self.state.clone(),
            changes: changes.clone(),
        });
        self.redo.clear();
        changes
    }

    pub(super) fn replace_selection_or_insert(&mut self, text: &str) -> Option<AppliedRange> {
        if text.is_empty() && self.selection_bytes().is_none() {
            return None;
        }
        let (start, end) = self
            .selection_bytes()
            .unwrap_or((self.state.cursor_byte, self.state.cursor_byte));
        Some(self.replace_byte_range(start, end, text))
    }

    pub(super) fn delete_back(&mut self) -> Option<AppliedRange> {
        if let Some((start, end)) = self.selection_bytes() {
            return Some(self.replace_byte_range(start, end, ""));
        }
        let content = self.content();
        let previous = text::previous_boundary(&content, self.state.cursor_byte)?;
        Some(self.replace_byte_range(previous, self.state.cursor_byte, ""))
    }

    pub(super) fn delete_forward(&mut self) -> Option<AppliedRange> {
        if let Some((start, end)) = self.selection_bytes() {
            return Some(self.replace_byte_range(start, end, ""));
        }
        let content = self.content();
        let next = text::next_boundary(&content, self.state.cursor_byte)?;
        Some(self.replace_byte_range(self.state.cursor_byte, next, ""))
    }

    pub(super) fn delete_logical_line(&mut self) -> Option<AppliedRange> {
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
        (start != end).then(|| self.replace_byte_range(start, end, ""))
    }

    pub(super) fn undo_edit(&mut self) -> TextChangeSet {
        let Some(entry) = self.undo.pop() else {
            return TextChangeSet::unchanged(self.state.text.len_bytes());
        };
        let changes = entry.changes.inverse();
        self.state = entry.before.clone();
        self.redo.push(entry);
        self.finish_history_move();
        changes
    }

    pub(super) fn redo_edit(&mut self) -> TextChangeSet {
        let Some(entry) = self.redo.pop() else {
            return TextChangeSet::unchanged(self.state.text.len_bytes());
        };
        let changes = entry.changes.clone();
        self.state = entry.after.clone();
        self.undo.push(entry);
        self.finish_history_move();
        changes
    }

    pub(super) fn replace_byte_range(
        &mut self,
        start: usize,
        end: usize,
        replacement: &str,
    ) -> AppliedRange {
        let start_char = self.state.text.byte_to_char(start);
        let end_char = self.state.text.byte_to_char(end);
        self.state.text.remove(start_char..end_char);
        self.state.text.insert(start_char, replacement);
        self.state.cursor_byte = start + replacement.len();
        self.state.selection_anchor_byte = None;
        AppliedRange {
            old: start..end,
            new: start..start + replacement.len(),
        }
    }

    fn finish_history_move(&mut self) {
        self.preferred_column = None;
        self.pointer_selection = None;
        self.ensure_cursor_visible();
    }
}
