//! Shared bounded single-line editor for transient text owners.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

use crate::ports::editor::CursorMovement;

const DEFAULT_MAX_BYTES: usize = 4 * 1024;
const MAX_HISTORY_ENTRIES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditClass {
    Typing,
    Backspace,
    Delete,
    Paste,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct QueryState {
    text: String,
    head: usize,
    anchor: Option<usize>,
}

/// Directional selection owned by one transient text field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct QuerySelection {
    /// Fixed endpoint from which the selection was extended.
    pub(in crate::ui) anchor: usize,
    /// Moving cursor endpoint.
    pub(in crate::ui) head: usize,
}

/// Bounded single-line editor with owner-local undo and redo.
#[derive(Clone, Debug)]
pub(in crate::ui) struct QueryEditor {
    state: QueryState,
    past: Vec<QueryState>,
    future: Vec<QueryState>,
    open_group: Option<EditClass>,
    maximum_bytes: usize,
    maximum_characters: Option<usize>,
}

impl Default for QueryEditor {
    fn default() -> Self {
        Self::with_limit(DEFAULT_MAX_BYTES)
    }
}

impl QueryEditor {
    pub(in crate::ui) fn with_limit(maximum_bytes: usize) -> Self {
        Self {
            state: QueryState::default(),
            past: Vec::new(),
            future: Vec::new(),
            open_group: None,
            maximum_bytes,
            maximum_characters: None,
        }
    }

    pub(in crate::ui) fn with_character_limit(maximum_characters: usize) -> Self {
        Self {
            maximum_characters: Some(maximum_characters),
            maximum_bytes: maximum_characters.saturating_mul(4),
            ..Self::with_limit(DEFAULT_MAX_BYTES)
        }
    }

    pub(in crate::ui) fn from_text(value: &str) -> Self {
        let mut editor = Self::default();
        let end = floor_boundary(value, editor.maximum_bytes);
        editor.state.text.push_str(&value[..end]);
        editor.state.head = end;
        editor
    }

    pub(in crate::ui) fn from_text_with_character_limit(
        value: &str,
        maximum_characters: usize,
    ) -> Self {
        let mut editor = Self::with_character_limit(maximum_characters);
        let end = prefix_boundary(
            value,
            editor.maximum_bytes,
            editor.maximum_characters.unwrap_or(usize::MAX),
        );
        editor.state.text.push_str(&value[..end]);
        editor.state.head = end;
        editor
    }

    pub(in crate::ui) fn text(&self) -> &str {
        &self.state.text
    }

    pub(in crate::ui) const fn cursor(&self) -> usize {
        self.state.head
    }

    pub(in crate::ui) const fn selection(&self) -> Option<QuerySelection> {
        match self.state.anchor {
            Some(anchor) if anchor != self.state.head => Some(QuerySelection {
                anchor,
                head: self.state.head,
            }),
            _ => None,
        }
    }

    pub(in crate::ui) fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub(in crate::ui) fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub(in crate::ui) fn insert_char(&mut self, character: char) {
        let mut encoded = [0; 4];
        self.insert(
            character.encode_utf8(&mut encoded),
            EditClass::Typing,
            false,
        );
    }

    pub(in crate::ui) fn paste(&mut self, value: &str) {
        self.insert(&normalize(value), EditClass::Paste, true);
    }

    pub(in crate::ui) fn backspace(&mut self) {
        self.edit(EditClass::Backspace, false, |state, _, _| {
            if delete_selection(state) {
                return;
            }
            let previous = previous_boundary(&state.text, state.head);
            state.text.replace_range(previous..state.head, "");
            state.head = previous;
        });
    }

    pub(in crate::ui) fn delete(&mut self) {
        self.edit(EditClass::Delete, false, |state, _, _| {
            if delete_selection(state) {
                return;
            }
            let next = next_boundary(&state.text, state.head);
            state.text.replace_range(state.head..next, "");
        });
    }

    pub(in crate::ui) fn move_cursor_with_selection(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
    ) {
        self.close_group();
        let previous = self.state.head;
        let next = match movement {
            CursorMovement::GraphemeBack => previous_boundary(&self.state.text, previous),
            CursorMovement::GraphemeForward => next_boundary(&self.state.text, previous),
            CursorMovement::WordBack => {
                crate::ports::text_layout::word_back(&self.state.text, previous)
            }
            CursorMovement::WordForward => {
                crate::ports::text_layout::word_forward(&self.state.text, previous)
            }
            CursorMovement::LineStart | CursorMovement::DocumentStart => 0,
            CursorMovement::LineEnd | CursorMovement::DocumentEnd => self.state.text.len(),
            CursorMovement::VisualUp
            | CursorMovement::VisualDown
            | CursorMovement::VisualJumpUp
            | CursorMovement::VisualJumpDown => previous,
        };
        if extend_selection {
            self.state.anchor.get_or_insert(previous);
            self.state.head = next;
            if self.state.anchor == Some(self.state.head) {
                self.state.anchor = None;
            }
        } else {
            self.state.head = next;
            self.state.anchor = None;
        }
    }

    pub(in crate::ui) fn select_all(&mut self) {
        self.close_group();
        self.state.anchor = (!self.state.text.is_empty()).then_some(0);
        self.state.head = self.state.text.len();
    }

    pub(in crate::ui) fn undo(&mut self) -> bool {
        self.close_group();
        let Some(previous) = self.past.pop() else {
            return false;
        };
        self.future
            .push(std::mem::replace(&mut self.state, previous));
        true
    }

    pub(in crate::ui) fn redo(&mut self) -> bool {
        self.close_group();
        let Some(next) = self.future.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.state, next);
        self.push_past(current);
        true
    }

    fn insert(&mut self, value: &str, class: EditClass, isolated: bool) {
        self.edit(
            class,
            isolated,
            |state, maximum_bytes, maximum_characters| {
                let range = selected_range(state);
                let retained = state.text.len().saturating_sub(range.len());
                let available = maximum_bytes.saturating_sub(retained);
                let retained_characters = state.text[..range.start]
                    .chars()
                    .count()
                    .saturating_add(state.text[range.end..].chars().count());
                let available_characters = maximum_characters
                    .unwrap_or(usize::MAX)
                    .saturating_sub(retained_characters);
                let end = prefix_boundary(value, available, available_characters);
                state.text.replace_range(range.clone(), &value[..end]);
                state.head = range.start.saturating_add(end);
                state.anchor = None;
            },
        );
    }

    fn edit(
        &mut self,
        class: EditClass,
        isolated: bool,
        update: impl FnOnce(&mut QueryState, usize, Option<usize>),
    ) {
        let before = self.state.clone();
        update(&mut self.state, self.maximum_bytes, self.maximum_characters);
        if self.state == before {
            return;
        }
        let joins_open_group = !isolated && self.open_group == Some(class);
        if !joins_open_group {
            self.push_past(before);
        }
        self.future.clear();
        self.open_group = (!isolated).then_some(class);
    }

    fn close_group(&mut self) {
        self.open_group = None;
    }

    fn push_past(&mut self, state: QueryState) {
        if self.past.len() == MAX_HISTORY_ENTRIES {
            self.past.remove(0);
        }
        self.past.push(state);
    }
}

fn selected_range(state: &QueryState) -> Range<usize> {
    let anchor = state.anchor.unwrap_or(state.head);
    anchor.min(state.head)..anchor.max(state.head)
}

fn delete_selection(state: &mut QueryState) -> bool {
    let range = selected_range(state);
    if range.is_empty() {
        return false;
    }
    state.text.replace_range(range.clone(), "");
    state.head = range.start;
    state.anchor = None;
    true
}

fn normalize(value: &str) -> String {
    value
        .replace("\r\n", " ")
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}' => ' ',
            value if value.is_control() => ' ',
            value => value,
        })
        .collect()
}

fn previous_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn next_boundary(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .grapheme_indices(true)
        .nth(1)
        .map_or(text.len(), |(index, _)| cursor + index)
}

fn floor_boundary(value: &str, maximum: usize) -> usize {
    let mut end = maximum.min(value.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    end
}

fn prefix_boundary(value: &str, maximum_bytes: usize, maximum_characters: usize) -> usize {
    let byte_end = floor_boundary(value, maximum_bytes);
    value[..byte_end]
        .char_indices()
        .nth(maximum_characters)
        .map_or(byte_end, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use crate::ports::editor::CursorMovement;

    use super::{QueryEditor, QuerySelection};

    #[test]
    fn paste_is_single_line_bounded_and_grapheme_safe() {
        let mut editor = QueryEditor::default();
        editor.paste("Grüße\r\n第二行\u{2028}done");
        assert_eq!(editor.text(), "Grüße 第二行 done");
        editor.backspace();
        assert_eq!(editor.text(), "Grüße 第二行 don");
        assert!(editor.undo());
        assert_eq!(editor.text(), "Grüße 第二行 done");
        assert!(editor.undo());
        assert_eq!(editor.text(), "");
        assert!(editor.redo());
        assert_eq!(editor.text(), "Grüße 第二行 done");
    }

    #[test]
    fn typing_and_deletion_coalesce_by_intention_class() {
        let mut editor = QueryEditor::default();
        for character in "three".chars() {
            editor.insert_char(character);
        }
        assert!(editor.undo());
        assert_eq!(editor.text(), "");
        assert!(editor.redo());
        editor.backspace();
        editor.backspace();
        assert_eq!(editor.text(), "thr");
        assert!(editor.undo());
        assert_eq!(editor.text(), "three");
    }

    #[test]
    fn directional_selection_and_cursor_round_trip_through_history() {
        let mut editor = QueryEditor::from_text("a界e\u{301}");
        editor.move_cursor_with_selection(CursorMovement::GraphemeBack, true);
        editor.move_cursor_with_selection(CursorMovement::GraphemeBack, true);
        assert_eq!(
            editor.selection(),
            Some(QuerySelection {
                anchor: "a界e\u{301}".len(),
                head: 1,
            })
        );
        editor.insert_char('X');
        assert_eq!(editor.text(), "aX");
        assert!(editor.undo());
        assert_eq!(editor.text(), "a界e\u{301}");
        assert_eq!(
            editor.selection(),
            Some(QuerySelection {
                anchor: "a界e\u{301}".len(),
                head: 1,
            })
        );
        assert!(editor.redo());
        assert_eq!(editor.text(), "aX");
    }

    #[test]
    fn unicode_word_navigation_preserves_directional_selection_in_history() {
        let mut editor = QueryEditor::from_text("alpha beta 界");
        editor.move_cursor_with_selection(CursorMovement::WordBack, false);
        assert_eq!(editor.cursor(), "alpha beta ".len());
        editor.move_cursor_with_selection(CursorMovement::WordBack, true);
        assert_eq!(
            editor.selection(),
            Some(QuerySelection {
                anchor: "alpha beta ".len(),
                head: "alpha ".len(),
            })
        );
        editor.insert_char('X');
        assert_eq!(editor.text(), "alpha X界");
        assert!(editor.undo());
        assert_eq!(editor.text(), "alpha beta 界");
        assert_eq!(
            editor.selection(),
            Some(QuerySelection {
                anchor: "alpha beta ".len(),
                head: "alpha ".len(),
            })
        );
        assert!(editor.redo());
        assert_eq!(editor.text(), "alpha X界");
    }

    #[test]
    fn divergent_input_clears_redo_and_reopen_starts_clean() {
        let mut editor = QueryEditor::default();
        editor.paste("one");
        editor.paste(" two");
        assert!(editor.undo());
        editor.insert_char('!');
        assert!(!editor.can_redo());

        let reopened = QueryEditor::from_text(editor.text());
        assert!(!reopened.can_undo());
        assert!(!reopened.can_redo());
    }
}
