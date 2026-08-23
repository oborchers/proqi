//! Logical text and grapheme navigation helpers.

use unicode_segmentation::UnicodeSegmentation;

use super::LogicalLine;
use crate::domain::TextPosition;

pub(super) fn logical_lines(content: &str) -> Vec<LogicalLine> {
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

pub(super) fn byte_for_position(content: &str, position: TextPosition) -> usize {
    let lines = logical_lines(content);
    let line = lines[position.line.min(lines.len().saturating_sub(1))];
    let text = &content[line.start..line.content_end];
    text.grapheme_indices(true)
        .nth(position.grapheme)
        .map_or(line.content_end, |(offset, _)| line.start + offset)
}

pub(super) fn position_for_byte(content: &str, byte: usize) -> TextPosition {
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
