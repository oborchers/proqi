//! Logical text and grapheme navigation helpers.

use unicode_segmentation::UnicodeSegmentation;

use crate::ports::text_layout::{logical_lines, position_for_byte};

pub(super) fn previous_boundary(content: &str, cursor: usize) -> Option<usize> {
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

pub(super) fn next_boundary(content: &str, cursor: usize) -> Option<usize> {
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

pub(super) fn word_range(content: &str, cursor: usize) -> Option<(usize, usize)> {
    content
        .split_word_bound_indices()
        .filter_map(|(start, segment)| {
            segment
                .unicode_words()
                .next()
                .map(|_| (start, start + segment.len()))
        })
        .find(|(start, end)| *start <= cursor && cursor < *end)
}

pub(super) fn grapheme_range(content: &str, cursor: usize) -> (usize, usize) {
    let start = cursor.min(content.len());
    let end = next_boundary(content, start).unwrap_or(start);
    (start, end)
}

pub(super) fn preferred_newline(content: &str, cursor: usize) -> &'static str {
    let cursor = cursor.min(content.len());
    if content[cursor..].starts_with("\r\n") {
        "\r\n"
    } else if content[cursor..].starts_with('\n') {
        "\n"
    } else if let Some(newline) = content[..cursor].rfind('\n') {
        if newline > 0 && content.as_bytes()[newline - 1] == b'\r' {
            "\r\n"
        } else {
            "\n"
        }
    } else if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}
