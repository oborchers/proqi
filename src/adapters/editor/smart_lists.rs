//! Conservative, exact Markdown list continuation and indentation.

use std::ops::Range;

use crate::ports::{
    editor::TextChangeSet,
    text_layout::{LogicalLine, logical_lines},
};

use super::{RopeEditor, mutation::AppliedRange};

#[derive(Clone, Copy)]
struct ListMarker<'a> {
    indentation: &'a str,
    marker: Marker<'a>,
    spacing: &'a str,
    task_spacing: Option<&'a str>,
    content: &'a str,
}

#[derive(Clone, Copy)]
enum Marker<'a> {
    Bullet(char),
    Ordered { number: &'a str, delimiter: char },
}

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
        if marker.content.trim_matches([' ', '\t']).is_empty() {
            if marker.indentation.is_empty() {
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
            let at = lines[line_index].start + marker.indentation.len();
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
                        let at = lines[line_index].start + marker.indentation.len();
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

impl ListMarker<'_> {
    fn continuation(&self) -> String {
        let marker = match self.marker {
            Marker::Bullet(bullet) => bullet.to_string(),
            Marker::Ordered { number, delimiter } => {
                number
                    .parse::<u64>()
                    .map_or_else(|_| number.to_owned(), |number| (number + 1).to_string())
                    + &delimiter.to_string()
            }
        };
        let task = self
            .task_spacing
            .map_or_else(String::new, |spacing| format!("[ ]{spacing}"));
        format!("{}{marker}{}{task}", self.indentation, self.spacing)
    }

    fn indentation_columns(&self) -> usize {
        indentation_columns(self.indentation)
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
    let marker = parse_marker(line_text)?;
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
    let marker = parse_marker(line_text)?;
    let indentation_end = line.start + marker.indentation.len();
    if is_thematic_break(line_text) || inside_fenced_code(content, line.start) {
        return None;
    }
    let has_indentation_unit = marker.indentation.contains('\t')
        || whitespace_suffix_range(content, line.start, indentation_end, width).is_some();
    if !has_indentation_unit || !has_adjacent_list_context(content, lines, line_index) {
        return None;
    }
    Some(marker)
}

pub(super) fn recognized_prefix_len(
    content: &str,
    lines: &[LogicalLine],
    line_index: usize,
    width: u8,
) -> Option<usize> {
    let marker = indentation_marker_at(content, lines, line_index, width)?;
    let line = *lines.get(line_index)?;
    let line_text = &content[line.start..line.content_end];
    Some(line_text.len().saturating_sub(marker.content.len()))
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
    let marker = parse_marker(line_text)?;
    let indentation_end = line.start + marker.indentation.len();
    if is_thematic_break(line_text)
        || inside_fenced_code(content, line.start)
        || whitespace_suffix_range(content, line.start, indentation_end, width).is_none()
        || !has_outdent_list_context(content, lines, line_index, marker.indentation, width)
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
        if parse_marker(text).is_some_and(|peer| {
            peer.indentation == indentation
                && !is_thematic_break(text)
                && !inside_fenced_code(content, line.start)
                && (peer.indentation.contains('\t')
                    || whitespace_suffix_range(
                        content,
                        line.start,
                        line.start + peer.indentation.len(),
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
    if marker.indentation.contains('\t') {
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
    let indentation_end = line.start + marker.indentation.len();
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

fn parse_marker(line: &str) -> Option<ListMarker<'_>> {
    let indentation_end = line
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
        .unwrap_or(line.len());
    let indentation = &line[..indentation_end];
    let rest = &line[indentation_end..];
    let (marker, marker_end) = parse_base_marker(rest)?;
    let spacing_end = marker_end + whitespace_prefix(&rest[marker_end..]);
    if spacing_end == marker_end {
        return None;
    }
    let spacing = &rest[marker_end..spacing_end];
    let after_marker = &rest[spacing_end..];
    let (task_spacing, content) =
        parse_task(after_marker).map_or((None, after_marker), |parsed| (Some(parsed.0), parsed.1));
    Some(ListMarker {
        indentation,
        marker,
        spacing,
        task_spacing,
        content,
    })
}

fn parse_base_marker(rest: &str) -> Option<(Marker<'_>, usize)> {
    let first = rest.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        return Some((Marker::Bullet(first), first.len_utf8()));
    }
    let digit_end = rest.bytes().take(9).take_while(u8::is_ascii_digit).count();
    if digit_end == 0
        || rest
            .as_bytes()
            .get(digit_end)
            .is_some_and(u8::is_ascii_digit)
    {
        return None;
    }
    let delimiter = rest[digit_end..].chars().next()?;
    matches!(delimiter, '.' | ')').then_some((
        Marker::Ordered {
            number: &rest[..digit_end],
            delimiter,
        },
        digit_end + delimiter.len_utf8(),
    ))
}

fn parse_task(after_marker: &str) -> Option<(&str, &str)> {
    let task = after_marker.get(..3)?;
    if !matches!(task, "[ ]" | "[x]" | "[X]") {
        return None;
    }
    let spacing_end = 3 + whitespace_prefix(&after_marker[3..]);
    (spacing_end > 3).then_some((&after_marker[3..spacing_end], &after_marker[spacing_end..]))
}

fn whitespace_prefix(value: &str) -> usize {
    value
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
        .unwrap_or(value.len())
}

fn indentation_columns(indentation: &str) -> usize {
    indentation.chars().fold(0, |column, character| {
        let mut buffer = [0; 4];
        column
            + crate::ports::text_layout::grapheme_cell_width(
                character.encode_utf8(&mut buffer),
                column,
            )
    })
}

fn is_thematic_break(line: &str) -> bool {
    let compact = line
        .trim_matches([' ', '\t'])
        .chars()
        .filter(|character| !matches!(character, ' ' | '\t'))
        .collect::<String>();
    compact.len() >= 3
        && compact.chars().next().is_some_and(|marker| {
            matches!(marker, '-' | '*') && compact.chars().all(|c| c == marker)
        })
}

fn inside_fenced_code(content: &str, current_start: usize) -> bool {
    let mut open: Option<(char, usize)> = None;
    for line in logical_lines(content)
        .into_iter()
        .take_while(|line| line.start < current_start)
    {
        let text = &content[line.start..line.content_end];
        let Some((marker, count, trailing)) = fence(text) else {
            continue;
        };
        match open {
            None => open = Some((marker, count)),
            Some((open_marker, minimum))
                if marker == open_marker
                    && count >= minimum
                    && trailing.trim_matches([' ', '\t']).is_empty() =>
            {
                open = None;
            }
            Some(_) => {}
        }
    }
    open.is_some()
}

fn fence(line: &str) -> Option<(char, usize, &str)> {
    let indentation = whitespace_prefix(line);
    let prefix = &line[..indentation];
    if indentation_columns(prefix) > 3 {
        return None;
    }
    let rest = &line[indentation..];
    let marker = rest.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = rest
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (count >= 3).then_some((marker, count, &rest[count..]))
}
