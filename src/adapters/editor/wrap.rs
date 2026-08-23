//! Visual wrapping and cell geometry helpers.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{WrappedLine, text::logical_lines};
use crate::ports::editor::VisualLine;

pub(super) fn wrap_content(content: &str, width: usize) -> Vec<WrappedLine> {
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
            let (end_index, cells) = segment_end(&graphemes, start_index, width);
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
                    text: display_text(&graphemes[start_index..end_index]),
                },
                start_byte: line.start + start_offset,
                end_byte: line.start + end_offset,
            });
            start_index = end_index;
        }
    }
    output
}

fn segment_end(graphemes: &[(usize, &str)], start: usize, width: usize) -> (usize, usize) {
    let mut end = start;
    let mut cells = 0;
    while end < graphemes.len() {
        let grapheme_width = grapheme_width(graphemes[end].1, cells);
        if end > start && cells + grapheme_width > width {
            break;
        }
        cells += grapheme_width;
        end += 1;
        if cells >= width {
            break;
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

pub(super) fn byte_at_cell(content: &str, line: &WrappedLine, target_cell: usize) -> usize {
    let text = &content[line.start_byte..line.end_byte];
    let mut cells = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let width = grapheme_width(grapheme, cells);
        if target_cell < cells + width {
            return line.start_byte + offset;
        }
        cells += width;
    }
    line.end_byte
}

pub(super) fn cell_column_at_byte(content: &str, line: &WrappedLine, byte: usize) -> usize {
    let end = byte.min(line.end_byte);
    content[line.start_byte..end]
        .graphemes(true)
        .fold(0, |cells, grapheme| cells + grapheme_width(grapheme, cells))
}

pub(super) fn wrapped_line_index(lines: &[WrappedLine], byte: usize) -> usize {
    lines
        .iter()
        .position(|line| {
            (line.start_byte == line.end_byte && byte == line.start_byte)
                || (byte >= line.start_byte && byte < line.end_byte)
        })
        .or_else(|| lines.iter().rposition(|line| line.start_byte <= byte))
        .unwrap_or(0)
}
