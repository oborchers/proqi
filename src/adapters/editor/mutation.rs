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
        let after_content = self.content();
        let changes = TextChange::new(&before_content, &after_content, applied.old, applied.new)
            .and_then(|change| TextChangeSet::new(&before_content, &after_content, vec![change]))
            .ok();
        self.finish_mutation(before_state, &before_content, changes)
    }

    pub(super) fn mutate_many(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Option<TextChangeSet>,
    ) -> TextChangeSet {
        let before_state = self.state.clone();
        let before_content = self.content();
        let changes = operation(self);
        self.finish_mutation(before_state, &before_content, changes)
    }

    fn finish_mutation(
        &mut self,
        before_state: State,
        before_content: &str,
        changes: Option<TextChangeSet>,
    ) -> TextChangeSet {
        self.preferred_column = None;
        self.pointer_selection = None;
        self.ensure_cursor_visible();
        let Some(changes) = changes.filter(|changes| !changes.is_empty()) else {
            if before_content != self.content() {
                self.state = before_state;
                self.ensure_cursor_visible();
            }
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

    pub(super) fn replace_byte_ranges(
        &mut self,
        replacements: &[(Range<usize>, String)],
    ) -> Option<TextChangeSet> {
        if replacements.is_empty() {
            return None;
        }
        let before = self.content();
        let mut previous_end = 0;
        for (range, _) in replacements {
            if range.start < previous_end
                || range.start > range.end
                || range.end > before.len()
                || !before.is_char_boundary(range.start)
                || !before.is_char_boundary(range.end)
            {
                return None;
            }
            previous_end = range.end;
        }
        let mut after = before.clone();
        for (range, replacement) in replacements.iter().rev() {
            after.replace_range(range.clone(), replacement);
        }
        let mut old_cursor = 0;
        let mut new_cursor = 0;
        let mut entries = Vec::with_capacity(replacements.len());
        for (range, replacement) in replacements {
            if range.start < old_cursor {
                return None;
            }
            new_cursor += range.start - old_cursor;
            let new_range = new_cursor..new_cursor + replacement.len();
            entries.push(TextChange::new(&before, &after, range.clone(), new_range).ok()?);
            old_cursor = range.end;
            new_cursor += replacement.len();
        }
        let changes = TextChangeSet::new(&before, &after, entries).ok()?;
        let cursor = changes
            .map_old_offset(
                &before,
                self.state.cursor_byte,
                crate::ports::editor::OffsetAffinity::After,
            )
            .ok()?;
        let anchor = match self.state.selection_anchor_byte {
            Some(anchor) => Some(
                changes
                    .map_old_offset(&before, anchor, crate::ports::editor::OffsetAffinity::After)
                    .ok()?,
            ),
            None => None,
        };
        self.state.text = ropey::Rope::from_str(&after);
        self.state.cursor_byte = cursor;
        self.state.selection_anchor_byte = anchor;
        Some(changes)
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

    pub(super) fn delete_sentence(&mut self, list_indent_width: u8) -> Option<TextChangeSet> {
        let content = self.content();
        let ranges = super::sentence::deletion_ranges(
            &content,
            self.state.cursor_byte,
            self.selection_bytes(),
            list_indent_width,
        );
        let cursor = ranges.first()?.start;
        let replacements = ranges
            .into_iter()
            .map(|range| (range, String::new()))
            .collect::<Vec<_>>();
        let changes = self.replace_byte_ranges(&replacements)?;
        self.state.cursor_byte = cursor;
        self.state.selection_anchor_byte = None;
        Some(changes)
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
