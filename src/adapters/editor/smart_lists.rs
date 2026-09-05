//! Conservative, exact Markdown list continuation and indentation.

mod scan;

use std::ops::Range;

use crate::ports::{
    editor::TextChangeSet,
    structured_text::{
        FenceState, ListMarker, is_thematic_break, parse_list_marker, whitespace_prefix,
    },
    text_layout::{LogicalLine, logical_lines},
};

use super::{RopeEditor, mutation::AppliedRange};

pub(super) use scan::recognized_prefix_lengths;

impl RopeEditor {
    pub(super) fn insert_smart_newline(&mut self, indent_width: u8) -> Option<AppliedRange> {
        let content = self.content();
        let newline = super::text::preferred_newline(&content, self.state.cursor_byte);
        if self.selection_bytes().is_some() {
            return self.replace_selection_or_insert(newline);
        }
        let cursor = self.state.cursor_byte;
        let lines = logical_lines(&content);
        let Some(line_index) = line_index_at(&lines, cursor) else {
            return self.replace_selection_or_insert(newline);
        };
        let line = lines[line_index];
        if cursor != line.content_end || inside_fenced_code(&content, line.start) {
            return self.replace_selection_or_insert(newline);
        }
        let Some(marker) = indentation_marker_at(&content, &lines, line_index, indent_width) else {
            return self.replace_selection_or_insert(newline);
        };
        if marker.content().trim_matches([' ', '\t']).is_empty() {
            if marker.indentation().is_empty() {
                return Some(self.replace_byte_range(line.start, cursor, ""));
            }
            if let Some(range) = outdent_range(&content, line, marker, indent_width) {
                let removed = range.len();
                let applied = self.replace_byte_range(range.start, range.end, "");
                self.state.cursor_byte = cursor.saturating_sub(removed);
                return Some(applied);
            }
        }
        let continuation = marker.continuation();
        Some(self.replace_byte_range(cursor, cursor, &format!("{newline}{continuation}")))
    }

    pub(super) fn indent(&mut self, width: u8, smart_lists: bool) -> Option<TextChangeSet> {
        let content = self.content();
        let indentation = " ".repeat(usize::from(width.max(1)));
        let lines = logical_lines(&content);
        let touched = touched_lines(&lines, self.state.cursor_byte, self.selection_bytes());
        if self.selection_bytes().is_none() {
            let line_index = touched[0];
            let Some(marker) = smart_lists
                .then(|| indentation_marker_at(&content, &lines, line_index, width))
                .flatten()
            else {
                return self.replace_byte_ranges(&[(
                    self.state.cursor_byte..self.state.cursor_byte,
                    indentation,
                )]);
            };
            let insert = indentation_unit(marker, width);
            let at = lines[line_index].start + marker.indentation().len();
            return self.replace_byte_ranges(&[(at..at, insert)]);
        }
        let replacements = touched
            .into_iter()
            .map(|line_index| {
                let marker = smart_lists
                    .then(|| indentation_marker_at(&content, &lines, line_index, width))
                    .flatten();
                marker.map_or_else(
                    || {
                        (
                            lines[line_index].start..lines[line_index].start,
                            indentation.clone(),
                        )
                    },
                    |marker| {
                        let at = lines[line_index].start + marker.indentation().len();
                        (at..at, indentation_unit(marker, width))
                    },
                )
            })
            .collect::<Vec<_>>();
        self.replace_byte_ranges(&replacements)
    }

    pub(super) fn outdent(&mut self, width: u8, smart_lists: bool) -> Option<TextChangeSet> {
        if !smart_lists {
            return None;
        }
        let content = self.content();
        let lines = logical_lines(&content);
        let touched = touched_lines(&lines, self.state.cursor_byte, self.selection_bytes());
        let markers = touched
            .iter()
            .map(|index| outdent_marker_at(&content, &lines, *index, width))
            .collect::<Vec<_>>();
        if !markers.iter().any(Option::is_some) {
            return None;
        }
        let replacements = touched
            .into_iter()
            .zip(markers)
            .filter_map(|(line_index, marker)| {
                let range = marker.map_or_else(
                    || ordinary_outdent_range(&content, lines[line_index], width),
                    |marker| outdent_range(&content, lines[line_index], marker, width),
                )?;
                Some((range, String::new()))
            })
            .collect::<Vec<_>>();
        self.replace_byte_ranges(&replacements)
    }
}

fn touched_lines(
    lines: &[LogicalLine],
    cursor: usize,
    selection: Option<(usize, usize)>,
) -> Vec<usize> {
    let Some((start, end)) = selection else {
        return vec![line_index_at(lines, cursor).unwrap_or(0)];
    };
    let first = line_index_at(lines, start).unwrap_or(0);
    let mut last = line_index_at(lines, end).unwrap_or(first);
    if last > first && end == lines[last].start {
        last -= 1;
    }
    (first..=last).collect()
}

fn line_index_at(lines: &[LogicalLine], byte: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .position(|(index, line)| byte < line.end || index + 1 == lines.len())
}

fn marker_at<'a>(
    content: &'a str,
    lines: &[LogicalLine],
    line_index: usize,
) -> Option<ListMarker<'a>> {
    let line = *lines.get(line_index)?;
    let line_text = &content[line.start..line.content_end];
    let marker = parse_list_marker(line_text)?;
    if is_thematic_break(line_text) || inside_fenced_code(content, line.start) {
        return None;
    }
    if marker.indentation_columns() <= 3 {
        Some(marker)
    } else {
        None
    }
}

fn indentation_marker_at<'a>(
    content: &'a str,
    lines: &[LogicalLine],
    line_index: usize,
    width: u8,
) -> Option<ListMarker<'a>> {
    if let Some(marker) = marker_at(content, lines, line_index) {
        return Some(marker);
    }
    let line = *lines.get(line_index)?;
    let line_text = &content[line.start..line.content_end];
    let marker = parse_list_marker(line_text)?;
    let indentation_end = line.start + marker.indentation().len();
    if is_thematic_break(line_text) || inside_fenced_code(content, line.start) {
        return None;
    }
    let has_indentation_unit = marker.indentation().contains('\t')
        || whitespace_suffix_range(content, line.start, indentation_end, width).is_some();
    if !has_indentation_unit || !has_adjacent_list_context(content, lines, line_index) {
        return None;
    }
    Some(marker)
}

fn outdent_marker_at<'a>(
    content: &'a str,
    lines: &[LogicalLine],
    line_index: usize,
    width: u8,
) -> Option<ListMarker<'a>> {
    if let Some(marker) = indentation_marker_at(content, lines, line_index, width) {
        return Some(marker);
    }
    let line = *lines.get(line_index)?;
    let line_text = &content[line.start..line.content_end];
    let marker = parse_list_marker(line_text)?;
    let indentation_end = line.start + marker.indentation().len();
    if is_thematic_break(line_text)
        || inside_fenced_code(content, line.start)
        || whitespace_suffix_range(content, line.start, indentation_end, width).is_none()
        || !has_outdent_list_context(content, lines, line_index, marker.indentation(), width)
    {
        return None;
    }
    Some(marker)
}

fn has_adjacent_list_context(content: &str, lines: &[LogicalLine], line_index: usize) -> bool {
    for index in (0..line_index).rev() {
        let line = lines[index];
        let text = &content[line.start..line.content_end];
        if text.trim_matches([' ', '\t']).is_empty() {
            return false;
        }
        if marker_at(content, lines, index).is_some() {
            return true;
        }
        if !text.starts_with([' ', '\t']) {
            return false;
        }
    }
    false
}

fn has_outdent_list_context(
    content: &str,
    lines: &[LogicalLine],
    line_index: usize,
    indentation: &str,
    width: u8,
) -> bool {
    let mut saw_peer = false;
    for index in (0..line_index).rev() {
        let line = lines[index];
        let text = &content[line.start..line.content_end];
        if text.trim_matches([' ', '\t']).is_empty() {
            return false;
        }
        if marker_at(content, lines, index).is_some() {
            return true;
        }
        if parse_list_marker(text).is_some_and(|peer| {
            peer.indentation() == indentation
                && !is_thematic_break(text)
                && !inside_fenced_code(content, line.start)
                && (peer.indentation().contains('\t')
                    || whitespace_suffix_range(
                        content,
                        line.start,
                        line.start + peer.indentation().len(),
                        width,
                    )
                    .is_some())
        }) {
            saw_peer = true;
            continue;
        }
        if !text.starts_with([' ', '\t']) {
            return false;
        }
    }
    line_index == 0 || saw_peer
}

fn indentation_unit(marker: ListMarker<'_>, width: u8) -> String {
    if marker.indentation().contains('\t') {
        "\t".to_owned()
    } else {
        " ".repeat(usize::from(width.max(1)))
    }
}

fn outdent_range(
    content: &str,
    line: LogicalLine,
    marker: ListMarker<'_>,
    width: u8,
) -> Option<Range<usize>> {
    let indentation_start = line.start;
    let indentation_end = line.start + marker.indentation().len();
    whitespace_suffix_range(content, indentation_start, indentation_end, width)
}

fn ordinary_outdent_range(content: &str, line: LogicalLine, width: u8) -> Option<Range<usize>> {
    let text = &content[line.start..line.content_end];
    let indentation_end = line.start + whitespace_prefix(text);
    whitespace_suffix_range(content, line.start, indentation_end, width)
}

fn whitespace_suffix_range(
    content: &str,
    start: usize,
    end: usize,
    width: u8,
) -> Option<Range<usize>> {
    let prefix = &content[start..end];
    if prefix.ends_with('\t') {
        return Some(end - 1..end);
    }
    let spaces = usize::from(width.max(1));
    (prefix.len() >= spaces
        && prefix[prefix.len() - spaces..]
            .bytes()
            .all(|byte| byte == b' '))
    .then(|| end - spaces..end)
}

fn inside_fenced_code(content: &str, current_start: usize) -> bool {
    let mut open = FenceState::closed();
    for line in logical_lines(content)
        .into_iter()
        .take_while(|line| line.start < current_start)
    {
        let text = &content[line.start..line.content_end];
        open.update(text);
    }
    open.is_open()
}
