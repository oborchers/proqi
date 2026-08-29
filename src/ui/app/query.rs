//! Shared bounded single-line editor for searchable overlays.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::ports::editor::CursorMovement;

const MAX_QUERY_BYTES: usize = 4 * 1024;

#[derive(Default)]
pub(super) struct QueryEditor {
    text: String,
    cursor: usize,
}

impl QueryEditor {
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) const fn cursor(&self) -> usize {
        self.cursor
    }

    pub(super) fn insert_char(&mut self, character: char) {
        let mut encoded = [0; 4];
        self.insert(character.encode_utf8(&mut encoded));
    }

    pub(super) fn paste(&mut self, value: &str) {
        self.insert(&normalize(value));
    }

    pub(super) fn backspace(&mut self) {
        let previous = previous_boundary(&self.text, self.cursor);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    pub(super) fn delete(&mut self) {
        let next = next_boundary(&self.text, self.cursor);
        self.text.replace_range(self.cursor..next, "");
    }

    pub(super) fn move_cursor(&mut self, movement: CursorMovement) {
        self.cursor = match movement {
            CursorMovement::GraphemeBack | CursorMovement::WordBack => {
                previous_boundary(&self.text, self.cursor)
            }
            CursorMovement::GraphemeForward | CursorMovement::WordForward => {
                next_boundary(&self.text, self.cursor)
            }
            CursorMovement::LineStart | CursorMovement::DocumentStart => 0,
            CursorMovement::LineEnd | CursorMovement::DocumentEnd => self.text.len(),
            CursorMovement::VisualUp
            | CursorMovement::VisualDown
            | CursorMovement::VisualJumpUp
            | CursorMovement::VisualJumpDown => self.cursor,
        };
    }

    fn insert(&mut self, value: &str) {
        let available = MAX_QUERY_BYTES.saturating_sub(self.text.len());
        let end = floor_boundary(value, available);
        self.text.insert_str(self.cursor, &value[..end]);
        self.cursor += end;
    }
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

#[cfg(test)]
mod tests {
    use super::QueryEditor;

    #[test]
    fn paste_is_single_line_bounded_and_grapheme_safe() {
        let mut editor = QueryEditor::default();
        editor.paste("Grüße\r\n第二行\u{2028}done");
        assert_eq!(editor.text(), "Grüße 第二行 done");
        editor.backspace();
        assert_eq!(editor.text(), "Grüße 第二行 don");
    }
}
