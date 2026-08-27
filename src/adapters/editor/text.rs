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

pub(super) fn word_range(content: &str, cursor: usize) -> Option<(usize, usize)> {
    word_segments(content)
        .into_iter()
        .find(|(start, end)| *start <= cursor && cursor < *end)
}

pub(super) fn grapheme_range(content: &str, cursor: usize) -> (usize, usize) {
    let start = cursor.min(content.len());
    let end = next_boundary(content, start).unwrap_or(start);
    (start, end)
}

pub(super) fn word_back(content: &str, cursor: usize) -> usize {
    word_segments(content)
        .into_iter()
        .rev()
        .find_map(|(start, _)| (start < cursor).then_some(start))
        .unwrap_or(0)
}

pub(super) fn word_forward(content: &str, cursor: usize) -> usize {
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
