//! Canonical grapheme and terminal-cell wrapping shared by editor and board UI.

use crate::domain::TextPosition;
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use super::editor::VisualLine;

const TAB_WIDTH: usize = 4;

pub(crate) fn grapheme_cell_width(grapheme: &str, column: usize) -> usize {
    if grapheme == "\t" {
        TAB_WIDTH - column % TAB_WIDTH
    } else if grapheme.chars().any(char::is_control) {
        1
    } else {
        grapheme.width()
    }
}

pub(crate) fn display_grapheme(grapheme: &str, column: usize) -> (String, usize) {
    let width = grapheme_cell_width(grapheme, column);
    let display = if grapheme == "\t" {
        " ".repeat(width)
    } else if grapheme.chars().any(char::is_control) {
        "�".to_owned()
    } else {
        grapheme.to_owned()
    };
    (display, width)
}

pub(crate) fn truncate_cells(value: &str, width: usize) -> String {
    let mut cells = 0_usize;
    let mut visible = String::new();
    for grapheme in value.graphemes(true) {
        let (display, grapheme_width) = display_grapheme(grapheme, cells);
        if cells.saturating_add(grapheme_width) > width {
            break;
        }
        visible.push_str(&display);
        cells = cells.saturating_add(grapheme_width);
    }
    visible
}

pub(crate) fn ellipsize_cells(value: &str, width: usize) -> String {
    if terminal_cell_width(value) <= width {
        return truncate_cells(value, width);
    }
    if width == 0 {
        return String::new();
    }
    let mut visible = truncate_cells(value, width.saturating_sub(1));
    visible.push('…');
    visible
}

pub(crate) fn terminal_cell_width(value: &str) -> usize {
    value.graphemes(true).fold(0, |cells, grapheme| {
        cells + grapheme_cell_width(grapheme, cells)
    })
}

/// Saturating terminal-cell width for Ratatui geometry.
pub(crate) fn terminal_cell_width_u16(value: &str) -> u16 {
    u16::try_from(terminal_cell_width(value)).unwrap_or(u16::MAX)
}

/// One rendered window over user-entered text and its cursor cell within that window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VisibleCellWindow {
    pub(crate) text: String,
    pub(crate) cursor_cell: usize,
}

/// Project a byte cursor to its terminal-cell column, clamping inside a grapheme backward.
pub(crate) fn cursor_cell(value: &str, cursor_byte: usize) -> usize {
    let cursor_byte = cursor_byte.min(value.len());
    let mut cells = 0_usize;
    for (byte, grapheme) in value.grapheme_indices(true) {
        if byte.saturating_add(grapheme.len()) > cursor_byte {
            break;
        }
        cells = cells.saturating_add(grapheme_cell_width(grapheme, cells));
    }
    cells
}

/// Keep the cursor visible in one sanitized terminal-cell-bounded text window.
pub(crate) fn visible_cell_window(
    value: &str,
    cursor_byte: usize,
    width: usize,
) -> VisibleCellWindow {
    if width == 0 {
        return VisibleCellWindow {
            text: String::new(),
            cursor_cell: 0,
        };
    }
    let cursor = cursor_cell(value, cursor_byte);
    let graphemes = display_graphemes(value);
    let mut start = 0;
    let mut start_cell = 0_usize;
    while start < graphemes.len() && cursor.saturating_sub(start_cell) > width {
        start_cell = start_cell.saturating_add(graphemes[start].1);
        start += 1;
    }
    let mut cells = 0_usize;
    let mut text = String::new();
    for (display, grapheme_width) in graphemes.into_iter().skip(start) {
        if cells.saturating_add(grapheme_width) > width {
            break;
        }
        text.push_str(&display);
        cells = cells.saturating_add(grapheme_width);
    }
    VisibleCellWindow {
        text,
        cursor_cell: cursor.saturating_sub(start_cell).min(width),
    }
}

fn display_graphemes(value: &str) -> Vec<(String, usize)> {
    let mut cells = 0_usize;
    value
        .graphemes(true)
        .map(|grapheme| {
            let display = display_grapheme(grapheme, cells);
            cells = cells.saturating_add(display.1);
            display
        })
        .collect()
}

#[derive(Clone, Copy)]
pub(crate) struct LogicalLine {
    pub(crate) start: usize,
    pub(crate) content_end: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct WrappedRow {
    pub(crate) visual: VisualLine,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
}

pub(crate) fn wrap_rows(content: &str, width: usize) -> Vec<WrappedRow> {
    let width = width.max(1);
    let mut output = Vec::new();
    for (logical_line, line) in logical_lines(content).into_iter().enumerate() {
        let text = &content[line.start..line.content_end];
        let graphemes: Vec<_> = text.grapheme_indices(true).collect();
        if graphemes.is_empty() {
            output.push(empty_row(logical_line, line.start, 0));
            continue;
        }
        let mut start = 0;
        while start < graphemes.len() {
            let (end, cells) = segment_end(&graphemes, start, width);
            let start_offset = graphemes[start].0;
            let end_offset = graphemes.get(end).map_or(text.len(), |(offset, _)| *offset);
            output.push(WrappedRow {
                visual: VisualLine {
                    start_byte: line.start + start_offset,
                    end_byte: line.start + end_offset,
                    logical_line,
                    start_grapheme: start,
                    end_grapheme: end,
                    cell_width: cells,
                    text: display_text(&graphemes[start..end]),
                    selected_cells: None,
                },
                start_byte: line.start + start_offset,
                end_byte: line.start + end_offset,
            });
            start = end;
        }
        if output.last().is_some_and(|row| {
            row.visual.logical_line == logical_line && row.visual.cell_width == width
        }) {
            output.push(empty_row(logical_line, line.content_end, graphemes.len()));
        }
    }
    output
}

pub(crate) fn byte_at_cell(content: &str, row: &WrappedRow, target: usize) -> usize {
    let text = &content[row.start_byte..row.end_byte];
    let mut cells = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let width = grapheme_cell_width(grapheme, cells);
        if target < cells + width {
            return row.start_byte + offset;
        }
        cells += width;
    }
    row.end_byte
}

pub(crate) fn cell_column_at_byte(content: &str, row: &WrappedRow, byte: usize) -> usize {
    let end = byte.min(row.end_byte);
    content[row.start_byte..end]
        .graphemes(true)
        .fold(0, |cells, grapheme| {
            cells + grapheme_cell_width(grapheme, cells)
        })
}

pub(crate) fn wrapped_row_index(rows: &[WrappedRow], byte: usize) -> usize {
    rows.iter()
        .position(|row| {
            (row.start_byte == row.end_byte && byte == row.start_byte)
                || (byte >= row.start_byte && byte < row.end_byte)
        })
        .or_else(|| rows.iter().rposition(|row| row.start_byte <= byte))
        .unwrap_or(0)
}

pub(crate) fn byte_for_position(content: &str, position: TextPosition) -> usize {
    let lines = logical_lines(content);
    let line = lines[position.line.min(lines.len().saturating_sub(1))];
    let text = &content[line.start..line.content_end];
    text.grapheme_indices(true)
        .nth(position.grapheme)
        .map_or(line.content_end, |(offset, _)| line.start + offset)
}

pub(crate) fn position_for_byte(content: &str, byte: usize) -> TextPosition {
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

fn segment_end(graphemes: &[(usize, &str)], start: usize, width: usize) -> (usize, usize) {
    let mut end = start;
    let mut cells = 0;
    let mut whitespace_break = None;
    while end < graphemes.len() {
        let grapheme_cells = grapheme_cell_width(graphemes[end].1, cells);
        if end > start && cells + grapheme_cells > width {
            return whitespace_break.unwrap_or((end, cells));
        }
        cells += grapheme_cells;
        end += 1;
        if breakable_whitespace(graphemes[end - 1].1) {
            whitespace_break = Some((end, cells));
        }
    }
    (end, cells)
}

fn breakable_whitespace(grapheme: &str) -> bool {
    grapheme.chars().all(char::is_whitespace)
        && !grapheme
            .chars()
            .any(|character| matches!(character, '\u{00a0}' | '\u{202f}'))
}

fn display_text(graphemes: &[(usize, &str)]) -> String {
    let mut display = String::new();
    let mut cells = 0;
    for (_, grapheme) in graphemes {
        let (visible, width) = display_grapheme(grapheme, cells);
        display.push_str(&visible);
        cells += width;
    }
    display
}

fn empty_row(logical_line: usize, byte: usize, grapheme: usize) -> WrappedRow {
    WrappedRow {
        visual: VisualLine {
            start_byte: byte,
            end_byte: byte,
            logical_line,
            start_grapheme: grapheme,
            end_grapheme: grapheme,
            cell_width: 0,
            text: String::new(),
            selected_cells: None,
        },
        start_byte: byte,
        end_byte: byte,
    }
}

pub(crate) fn logical_lines(content: &str) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (separator, character) in content
        .char_indices()
        .filter(|(_, character)| matches!(character, '\n' | '\u{2028}' | '\u{2029}'))
    {
        let content_end =
            if character == '\n' && separator > start && content.as_bytes()[separator - 1] == b'\r'
            {
                separator - 1
            } else {
                separator
            };
        lines.push(LogicalLine {
            start,
            content_end,
            end: separator + character.len_utf8(),
        });
        start = separator + character.len_utf8();
    }
    lines.push(LogicalLine {
        start,
        content_end: content.len(),
        end: content.len(),
    });
    lines
}

#[cfg(test)]
mod tests {
    use unicode_segmentation::UnicodeSegmentation as _;

    use crate::domain::TextPosition;

    use super::{
        LogicalLine, byte_for_position, cursor_cell, ellipsize_cells, logical_lines,
        position_for_byte, terminal_cell_width, truncate_cells, visible_cell_window, wrap_rows,
    };

    #[test]
    fn terminal_cell_helpers_share_tabs_controls_and_unicode_width() {
        let value = "a\te\u{301}界👩‍💻\u{7}";
        assert_eq!(terminal_cell_width(value), 10);
        assert_eq!(cursor_cell(value, "a\te\u{301}".len()), 5);
        assert_eq!(cursor_cell(value, "a\te".len()), 4);
        assert_eq!(truncate_cells(value, 7), "a   e\u{301}界");
        assert_eq!(truncate_cells(value, 10), "a   e\u{301}界👩‍💻�");
        assert_eq!(ellipsize_cells("\t\u{7}", 5), "    �");
    }

    #[test]
    fn visible_window_keeps_cursor_in_cells_and_never_splits_graphemes() {
        let value = "ab\t界e\u{301}👩‍💻\u{7}z";
        let at_emoji = "ab\t界e\u{301}👩‍💻".len();
        assert_eq!(
            visible_cell_window(value, at_emoji, 6),
            super::VisibleCellWindow {
                text: "界e\u{301}👩‍💻�".to_owned(),
                cursor_cell: 5,
            }
        );
        assert_eq!(
            visible_cell_window(value, "ab\t界e".len(), 4),
            super::VisibleCellWindow {
                text: "  界".to_owned(),
                cursor_cell: 4,
            }
        );
        assert_eq!(visible_cell_window(value, at_emoji, 0).cursor_cell, 0);
    }

    #[test]
    fn ordinary_words_wrap_at_the_latest_whitespace_boundary() {
        let rows = wrap_rows("Explain the smallest next step", 12);
        let visible = rows
            .iter()
            .map(|row| row.visual.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(visible, ["Explain the ", "smallest ", "next step"]);
        assert_eq!(rows[1].start_byte, "Explain the ".len());
    }

    #[test]
    fn oversized_unicode_tokens_still_hard_wrap_without_splitting_graphemes() {
        let rows = wrap_rows("界界界e\u{301}界", 4);
        assert_eq!(rows[0].visual.text, "界界");
        assert_eq!(rows[1].visual.text, "界e\u{301}");
        assert_eq!(rows[2].visual.text, "界");
        assert_eq!(rows[1].visual.start_grapheme, 2);
    }

    #[test]
    fn nonbreaking_spaces_do_not_become_wrap_boundaries() {
        let rows = wrap_rows("alpha\u{a0}beta gamma", 9);
        let visible = rows
            .iter()
            .map(|row| row.visual.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(visible, ["alpha\u{a0}bet", "a gamma"]);

        let narrow = wrap_rows("one\u{202f}two three", 7);
        assert_eq!(narrow[0].visual.text, "one\u{202f}two");
    }

    #[test]
    fn mandatory_unicode_separators_create_distinct_logical_rows() {
        let content = "one\u{2028}two\u{2029}three";
        let rows = wrap_rows(content, 80);
        let visible = rows
            .iter()
            .map(|row| row.visual.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(visible, ["one", "two", "three"]);
        assert_eq!(
            position_for_byte(content, "one\u{2028}".len()),
            TextPosition::new(1, 0)
        );
    }

    #[test]
    fn trailing_newlines_round_trip_as_distinct_logical_insertion_points() {
        let content = "line\n\n";
        for position in [
            TextPosition::new(0, 4),
            TextPosition::new(1, 0),
            TextPosition::new(2, 0),
        ] {
            let byte = byte_for_position(content, position);
            assert_eq!(position_for_byte(content, byte), position);
        }
    }

    #[test]
    fn every_logical_grapheme_position_round_trips_through_its_canonical_byte() {
        for content in ["", "line\n", "line\n\n", "e\u{301}\n界🙂\n", "one\r\ntwo"] {
            for (line_index, line) in logical_lines(content).into_iter().enumerate() {
                assert_line_positions_round_trip(content, line_index, line);
            }
        }
    }

    fn assert_line_positions_round_trip(content: &str, line_index: usize, line: LogicalLine) {
        let graphemes = content[line.start..line.content_end]
            .graphemes(true)
            .count();
        for grapheme in 0..=graphemes {
            let position = TextPosition::new(line_index, grapheme);
            let byte = byte_for_position(content, position);
            assert_eq!(position_for_byte(content, byte), position, "{content:?}");
        }
    }
}
