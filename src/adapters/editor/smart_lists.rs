//! Conservative, exact Markdown list continuation.

use crate::ports::text_layout::logical_lines;

use super::{RopeEditor, mutation::AppliedRange};

struct ListMarker<'a> {
    indentation: &'a str,
    marker: Marker<'a>,
    spacing: &'a str,
    task_spacing: Option<&'a str>,
    content: &'a str,
}

enum Marker<'a> {
    Bullet(char),
    Ordered { number: &'a str, delimiter: char },
}

impl RopeEditor {
    pub(super) fn insert_smart_newline(&mut self) -> Option<AppliedRange> {
        let content = self.content();
        let newline = preferred_newline(&content, self.state.cursor_byte);
        if self.selection_bytes().is_some() {
            return self.replace_selection_or_insert(newline);
        }
        let cursor = self.state.cursor_byte;
        let Some(line) = logical_lines(&content)
            .into_iter()
            .find(|line| cursor >= line.start && cursor <= line.content_end)
        else {
            return self.replace_selection_or_insert(newline);
        };
        if cursor != line.content_end || inside_fenced_code(&content, line.start) {
            return self.replace_selection_or_insert(newline);
        }
        let line_text = &content[line.start..line.content_end];
        let Some(marker) = parse_marker(line_text) else {
            return self.replace_selection_or_insert(newline);
        };
        if is_thematic_break(line_text) {
            return self.replace_selection_or_insert(newline);
        }
        if marker.indentation.is_empty() && marker.content.trim_matches([' ', '\t']).is_empty() {
            return Some(self.replace_byte_range(line.start, cursor, ""));
        }
        let continuation = marker.continuation();
        Some(self.replace_byte_range(cursor, cursor, &format!("{newline}{continuation}")))
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
}

fn parse_marker(line: &str) -> Option<ListMarker<'_>> {
    let indentation_end = line
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
        .unwrap_or(line.len());
    let indentation = &line[..indentation_end];
    if indentation_columns(indentation) > 3 {
        return None;
    }
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
        if character == '\t' {
            column + (4 - column % 4)
        } else {
            column + 1
        }
    })
}

fn preferred_newline(content: &str, cursor: usize) -> &'static str {
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
