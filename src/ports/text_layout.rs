//! Canonical grapheme and terminal-cell wrapping shared by editor and board UI.

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr;

use super::editor::VisualLine;

#[derive(Clone, Debug)]
pub(crate) struct WrappedRow {
    pub(crate) visual: VisualLine,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
}

pub(crate) fn wrap_rows(content: &str, width: usize) -> Vec<WrappedRow> {
    let width = width.max(1);
    let mut output = Vec::new();
    for (logical_line, line) in line_ranges(content).into_iter().enumerate() {
        let text = &content[line.start..line.end];
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
            output.push(empty_row(logical_line, line.end, graphemes.len()));
        }
    }
    output
}

pub(crate) fn byte_at_cell(content: &str, row: &WrappedRow, target: usize) -> usize {
    let text = &content[row.start_byte..row.end_byte];
    let mut cells = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let width = grapheme_width(grapheme, cells);
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
        .fold(0, |cells, grapheme| cells + grapheme_width(grapheme, cells))
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

fn segment_end(graphemes: &[(usize, &str)], start: usize, width: usize) -> (usize, usize) {
    let mut end = start;
    let mut cells = 0;
    let mut whitespace_break = None;
    while end < graphemes.len() {
        let grapheme_cells = grapheme_width(graphemes[end].1, cells);
        if end > start && cells + grapheme_cells > width {
            return whitespace_break.unwrap_or((end, cells));
        }
        cells += grapheme_cells;
        end += 1;
        if graphemes[end - 1].1.chars().all(char::is_whitespace) {
            whitespace_break = Some((end, cells));
        }
    }
    (end, cells)
}

fn grapheme_width(grapheme: &str, column: usize) -> usize {
    if grapheme == "\t" {
        4 - column % 4
    } else if grapheme.chars().any(char::is_control) {
        1
    } else {
        UnicodeWidthStr::width(grapheme)
    }
}

pub(crate) fn display_width(text: &str) -> usize {
    text.graphemes(true).fold(0, |column, grapheme| {
        column + grapheme_width(grapheme, column)
    })
}

fn display_text(graphemes: &[(usize, &str)]) -> String {
    let mut display = String::new();
    let mut cells = 0;
    for (_, grapheme) in graphemes {
        let width = grapheme_width(grapheme, cells);
        if *grapheme == "\t" {
            display.push_str(&" ".repeat(width));
        } else if grapheme.chars().any(char::is_control) {
            display.push('�');
        } else {
            display.push_str(grapheme);
        }
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

#[derive(Clone, Copy)]
struct LineRange {
    start: usize,
    end: usize,
}

fn line_ranges(content: &str) -> Vec<LineRange> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (newline, _) in content.match_indices('\n') {
        let end = if newline > start && content.as_bytes()[newline - 1] == b'\r' {
            newline - 1
        } else {
            newline
        };
        lines.push(LineRange { start, end });
        start = newline + 1;
    }
    lines.push(LineRange {
        start,
        end: content.len(),
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::wrap_rows;

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
}
