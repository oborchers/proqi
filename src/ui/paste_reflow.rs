//! Deterministic cleanup for explicitly requested plain-text paste reflow.

use std::ops::Range;

use crate::ports::{
    editor::{TextChange, TextChangeError, TextChangeSet},
    structured_text::{indentation_columns, parse_list_marker, whitespace_prefix},
    text_layout::{LogicalLine, logical_lines},
};

mod classify;
mod isolated;
#[cfg(test)]
mod tests;

use classify::LineKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReflowedText {
    pub(super) content: String,
    pub(super) changes: TextChangeSet,
    pub(super) isolated: Vec<(Range<usize>, Range<usize>)>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum ReflowError {
    #[error("text change mapping failed")]
    TextChange(#[from] TextChangeError),
    #[error("reflow boundary was invalid")]
    InvalidBoundary,
}

struct Replacement {
    old: Range<usize>,
    value: String,
}

#[cfg(test)]
pub(super) fn reflow_text(
    content: &str,
    protected: &[Range<usize>],
) -> Result<ReflowedText, ReflowError> {
    reflow_text_isolated(content, protected, &[])
}

pub(super) fn reflow_text_isolated(
    content: &str,
    protected: &[Range<usize>],
    isolated: &[Range<usize>],
) -> Result<ReflowedText, ReflowError> {
    isolated::reflow(content, protected, isolated)
}

fn reflow_slice(
    content: &str,
    protected: &[Range<usize>],
    newline: &str,
) -> Result<ReflowedText, ReflowError> {
    let lines = logical_lines(content);
    let kinds = classify::classify_lines(content, &lines, protected);
    let replacements = plan_replacements(content, &lines, &kinds, newline);
    apply_replacements(content, replacements)
}

fn unchanged(content: &str) -> ReflowedText {
    ReflowedText {
        content: content.to_owned(),
        changes: TextChangeSet::unchanged(content.len()),
        isolated: Vec::new(),
    }
}

fn contains_unsupported_control(content: &str) -> bool {
    content.char_indices().any(|(index, character)| {
        character.is_control() && !matches!(character, '\n' | '\r' | '\t')
            || matches!(character, '\u{2028}' | '\u{2029}')
            || (character == '\r' && content.as_bytes().get(index + 1) != Some(&b'\n'))
    })
}

fn preferred_newline(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn plan_replacements(
    content: &str,
    lines: &[LogicalLine],
    kinds: &[LineKind],
    newline: &str,
) -> Vec<Replacement> {
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let kind = kinds[index];
        let start = index;
        index += 1;
        while index < lines.len() && kinds[index] == kind {
            index += 1;
        }
        let range = replacement_range(content, lines, kinds, start, index, kind);
        if kind == LineKind::Protected {
            continue;
        }
        let value = match kind {
            LineKind::Separator => separator_value(kinds, start, index, newline),
            LineKind::ListParagraph(_) => {
                transform_indented_group(&content[range.clone()], newline)
            }
            LineKind::Reflow => transform_group(&content[range.clone()], newline),
            LineKind::Protected => continue,
        };
        if content[range.clone()] != value {
            replacements.push(Replacement { old: range, value });
        }
    }
    replacements
}

fn transform_indented_group(group: &str, _newline: &str) -> String {
    let lines = logical_lines(group);
    let prefix_end = lines.first().map_or(0, |line| {
        whitespace_prefix(&group[line.start..line.content_end])
    });
    let mut output = String::from(&group[..prefix_end]);
    output.push_str(
        &lines
            .iter()
            .map(|line| {
                let text = &group[line.start..line.content_end];
                collapse_inline(
                    &text[whitespace_prefix(text)..],
                    line.end > line.content_end,
                )
            })
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    );
    output
}

fn replacement_range(
    content: &str,
    lines: &[LogicalLine],
    kinds: &[LineKind],
    start: usize,
    end: usize,
    kind: LineKind,
) -> Range<usize> {
    if kind != LineKind::Separator {
        return lines[start].start..lines[end - 1].content_end;
    }
    let range_start = if start == 0 {
        0
    } else {
        lines[start - 1].content_end
    };
    let range_end = if end == kinds.len() {
        content.len()
    } else {
        lines[end].start
    };
    range_start..range_end
}

fn separator_value(kinds: &[LineKind], start: usize, end: usize, newline: &str) -> String {
    if start == 0 || end == kinds.len() {
        String::new()
    } else {
        newline.repeat(2)
    }
}

fn transform_group(group: &str, newline: &str) -> String {
    let lines = logical_lines(group);
    if lines
        .iter()
        .any(|line| parse_list_marker(&group[line.start..line.content_end]).is_some())
    {
        transform_list_group(group, &lines, newline)
    } else {
        lines
            .iter()
            .map(|line| {
                collapse_inline(
                    &group[line.start..line.content_end],
                    line.end > line.content_end,
                )
            })
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn transform_list_group(group: &str, lines: &[LogicalLine], newline: &str) -> String {
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let text = &group[lines[index].start..lines[index].content_end];
        let Some(marker) = parse_list_marker(text) else {
            let prose_start = index;
            while index < lines.len()
                && parse_list_marker(&group[lines[index].start..lines[index].content_end]).is_none()
            {
                index += 1;
            }
            output.push(
                lines[prose_start..index]
                    .iter()
                    .map(|line| {
                        collapse_inline(
                            &group[line.start..line.content_end],
                            line.end > line.content_end,
                        )
                    })
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            continue;
        };
        let mut item = String::from(&text[..marker.prefix_len()]);
        item.push_str(&collapse_inline(
            marker.content(),
            lines[index].end > lines[index].content_end,
        ));
        index += 1;
        while index < lines.len()
            && is_aligned_continuation(group, lines[index], marker.content_column())
        {
            let continuation = &group[lines[index].start..lines[index].content_end];
            let normalized = collapse_inline(
                &continuation[whitespace_prefix(continuation)..],
                lines[index].end > lines[index].content_end,
            );
            append_continuation(&mut item, &normalized);
            index += 1;
        }
        output.push(item);
    }
    output.join(newline)
}

fn append_continuation(item: &mut String, continuation: &str) {
    if continuation.is_empty() {
        return;
    }
    if !item.ends_with([' ', '\t']) {
        item.push(' ');
    }
    item.push_str(continuation);
}

fn is_aligned_continuation(group: &str, line: LogicalLine, column: usize) -> bool {
    let text = &group[line.start..line.content_end];
    parse_list_marker(text).is_none()
        && !text.trim_matches([' ', '\t']).is_empty()
        && indentation_columns(&text[..whitespace_prefix(text)]) == column
}

fn collapse_inline(line: &str, strip_hard_break: bool) -> String {
    let trimmed = line.trim_matches([' ', '\t']);
    let without_hard_break = if strip_hard_break {
        trimmed.strip_suffix('\\').unwrap_or(trimmed)
    } else {
        trimmed
    };
    let mut output = String::with_capacity(without_hard_break.len());
    let mut pending_space = false;
    for character in without_hard_break.chars() {
        if matches!(character, ' ' | '\t') {
            pending_space = !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
            }
            output.push(character);
            pending_space = false;
        }
    }
    output
}

fn apply_replacements(
    content: &str,
    replacements: Vec<Replacement>,
) -> Result<ReflowedText, ReflowError> {
    let capacity = replacements
        .iter()
        .fold(content.len(), |size, replacement| {
            size.saturating_sub(replacement.old.len())
                .saturating_add(replacement.value.len())
        });
    let mut output = String::with_capacity(capacity);
    let mut cursor = 0;
    let mut mapped = Vec::with_capacity(replacements.len());
    for replacement in replacements {
        output.push_str(&content[cursor..replacement.old.start]);
        let new_start = output.len();
        output.push_str(&replacement.value);
        mapped.push((replacement.old.clone(), new_start..output.len()));
        cursor = replacement.old.end;
    }
    output.push_str(&content[cursor..]);
    let changes = mapped
        .into_iter()
        .map(|(old, new)| TextChange::new(content, &output, old, new))
        .collect::<Result<Vec<_>, _>>()?;
    let changes = TextChangeSet::new(content, &output, changes)?;
    Ok(ReflowedText {
        content: output,
        changes,
        isolated: Vec::new(),
    })
}
