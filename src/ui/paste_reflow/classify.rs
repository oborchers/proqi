//! Conservative structural recognition for explicit paste reflow.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

use crate::ports::{
    structured_text::{
        FenceState, indentation_columns, is_thematic_break, parse_list_marker, whitespace_prefix,
    },
    text_layout::LogicalLine,
};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum LineKind {
    Protected,
    Reflow,
    ListParagraph(usize),
    Separator,
}

pub(super) fn classify_lines(
    content: &str,
    lines: &[LogicalLine],
    protected: &[Range<usize>],
) -> Vec<LineKind> {
    let mut fence = FenceState::closed();
    let mut kinds = Vec::with_capacity(lines.len());
    let mut list_content_column = None;
    let mut after_list_separator = false;
    let mut in_list_paragraph = false;
    let mut protected_index = 0;
    let mut indented_code = Vec::with_capacity(lines.len());
    for line in lines {
        let text = &content[line.start..line.content_end];
        let was_in_fence = fence.is_open();
        let is_fence = fence.update(text);
        let marker = parse_list_marker(text);
        while protected
            .get(protected_index)
            .is_some_and(|range| range.end <= line.start)
        {
            protected_index += 1;
        }
        let annotation_owned = protected
            .get(protected_index)
            .is_some_and(|range| ranges_intersect(&(line.start..line.end), range));
        let marker_owned = marker.is_some_and(|marker| {
            marker.indentation_columns() < 4 || list_content_column.is_some()
        });
        let aligned_continuation = list_content_column.is_some_and(|column| {
            marker.is_none()
                && !text.trim_matches([' ', '\t']).is_empty()
                && indentation_columns(&text[..whitespace_prefix(text)]) == column
        });
        let list_paragraph =
            marker.is_none() && aligned_continuation && (after_list_separator || in_list_paragraph);
        let indentation = indentation_columns(&text[..whitespace_prefix(text)]);
        indented_code.push(
            !text.trim_matches([' ', '\t']).is_empty()
                && indentation >= 4
                && !(marker_owned || aligned_continuation),
        );
        let mut kind = classify_line(
            content,
            *line,
            text,
            annotation_owned,
            was_in_fence || is_fence,
            marker_owned || aligned_continuation,
        );
        if kind == LineKind::Reflow
            && list_paragraph
            && let Some(column) = list_content_column
        {
            kind = LineKind::ListParagraph(column);
        }
        kinds.push(kind);
        if kind == LineKind::Separator {
            after_list_separator = list_content_column.is_some();
            in_list_paragraph = false;
        } else if marker_owned && let Some(marker) = marker {
            list_content_column = Some(marker.content_column());
            after_list_separator = false;
            in_list_paragraph = false;
        } else if list_paragraph {
            after_list_separator = false;
            in_list_paragraph = true;
        } else if aligned_continuation {
            after_list_separator = false;
            in_list_paragraph = false;
        } else {
            list_content_column = None;
            after_list_separator = false;
            in_list_paragraph = false;
        }
    }
    protect_indented_code_gaps(&mut kinds, &indented_code);
    protect_structural_blocks(content, lines, &mut kinds);
    kinds
}

fn protect_indented_code_gaps(kinds: &mut [LineKind], indented_code: &[bool]) {
    let mut start = 0;
    while start < kinds.len() {
        if kinds[start] != LineKind::Separator {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < kinds.len() && kinds[end] == LineKind::Separator {
            end += 1;
        }
        if start > 0 && end < kinds.len() && indented_code[start - 1] && indented_code[end] {
            kinds[start..end].fill(LineKind::Protected);
        }
        start = end;
    }
}

fn classify_line(
    content: &str,
    line: LogicalLine,
    text: &str,
    annotation_owned: bool,
    fenced: bool,
    list_owned: bool,
) -> LineKind {
    if annotation_owned {
        return LineKind::Protected;
    }
    if text.trim_matches([' ', '\t']).is_empty() && !fenced {
        return LineKind::Separator;
    }
    let indentation = indentation_columns(&text[..whitespace_prefix(text)]);
    let structural = fenced
        || text.trim_start_matches([' ', '\t']).starts_with('>')
        || is_atx_heading(text)
        || text.contains('|')
        || is_thematic_break(text)
        || (indentation >= 4 && !list_owned)
        || contains_path_or_url(text);
    if structural {
        LineKind::Protected
    } else if content.get(line.start..line.content_end).is_some() {
        LineKind::Reflow
    } else {
        LineKind::Protected
    }
}

fn protect_structural_blocks(content: &str, lines: &[LogicalLine], kinds: &mut [LineKind]) {
    let mut start = 0;
    while start < lines.len() {
        if kinds[start] == LineKind::Separator {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < lines.len() && kinds[end] != LineKind::Separator {
            end += 1;
        }
        let range = lines[start].start..lines[end - 1].content_end;
        if kinds[start..end].contains(&LineKind::Protected)
            || contains_path_or_url(&content[range])
            || looks_like_aligned_table(content, &lines[start..end])
            || looks_like_filename_list(content, &lines[start..end])
            || lines[start..end]
                .iter()
                .any(|line| is_setext_underline(&content[line.start..line.content_end]))
        {
            kinds[start..end].fill(LineKind::Protected);
        }
        start = end;
    }
}

fn looks_like_aligned_table(content: &str, lines: &[LogicalLine]) -> bool {
    let Some((first, rest)) = lines.split_first() else {
        return false;
    };
    let first_text = &content[first.start..first.content_end];
    let mut shared = aligned_field_starts(first_text)
        .into_iter()
        .map(|field| SharedField {
            column: field.column,
            first_left: field.left,
            first_right: field.right,
            distinct: false,
        })
        .collect::<Vec<_>>();
    for line in rest {
        let text = &content[line.start..line.content_end];
        let starts = aligned_field_starts(text);
        shared = intersect_fields(&shared, &starts, first_text, text);
        if shared.is_empty() {
            return false;
        }
    }
    lines.len() >= 2 && shared.into_iter().any(|field| field.distinct)
}

struct FieldStart {
    column: usize,
    left: Range<usize>,
    right: Range<usize>,
}

struct SharedField {
    column: usize,
    first_left: Range<usize>,
    first_right: Range<usize>,
    distinct: bool,
}

fn intersect_fields(
    left: &[SharedField],
    right: &[FieldStart],
    first_line: &str,
    current_line: &str,
) -> Vec<SharedField> {
    let mut shared = Vec::new();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].column.cmp(&right[right_index].column) {
            std::cmp::Ordering::Less => left_index += 1,
            std::cmp::Ordering::Greater => right_index += 1,
            std::cmp::Ordering::Equal => {
                let mut field = SharedField {
                    column: left[left_index].column,
                    first_left: left[left_index].first_left.clone(),
                    first_right: left[left_index].first_right.clone(),
                    distinct: left[left_index].distinct,
                };
                field.distinct |= first_line[field.first_left.clone()]
                    != current_line[right[right_index].left.clone()]
                    || first_line[field.first_right.clone()]
                        != current_line[right[right_index].right.clone()];
                shared.push(field);
                left_index += 1;
                right_index += 1;
            }
        }
    }
    shared
}

fn aligned_field_starts(line: &str) -> Vec<FieldStart> {
    let mut starts = Vec::new();
    let mut column = 0;
    let mut separator_width = 0;
    let mut separator_has_tab = false;
    let mut seen_content = false;
    let mut field_start = 0;
    let mut field_end = 0;
    let mut pending_right = None;
    for (index, grapheme) in line.grapheme_indices(true) {
        let width = crate::ports::text_layout::grapheme_cell_width(grapheme, column);
        if seen_content && matches!(grapheme, " " | "\t") {
            pending_right = None;
            separator_width += width;
            separator_has_tab |= grapheme == "\t";
        } else {
            if (separator_width >= 2 || separator_has_tab) && !grapheme.trim().is_empty() {
                starts.push(FieldStart {
                    column,
                    left: field_start..field_end,
                    right: index..index + grapheme.len(),
                });
                pending_right = Some(starts.len() - 1);
            }
            if separator_width > 0 || !seen_content {
                field_start = index;
            }
            if !grapheme.trim().is_empty() {
                field_end = index + grapheme.len();
                extend_pending_right(&mut starts, pending_right, field_end);
            }
            separator_width = 0;
            separator_has_tab = false;
            seen_content |= !grapheme.trim().is_empty();
        }
        column += width;
    }
    starts
}

fn extend_pending_right(starts: &mut [FieldStart], pending: Option<usize>, end: usize) {
    if let Some(pending) = pending {
        starts[pending].right.end = end;
    }
}

fn contains_path_or_url(line: &str) -> bool {
    line.split_whitespace().any(|word| {
        let token = word.trim_matches(|character: char| {
            matches!(
                character,
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '"' | '\''
            )
        });
        token.starts_with("http://")
            || token.starts_with("https://")
            || token.starts_with("file://")
            || token.starts_with('/') && token.len() > 1
            || token.starts_with("~/")
            || token.starts_with("./")
            || token.starts_with("../")
            || token.trim_end_matches('\\').contains('\\')
            || is_windows_path(token)
            || is_relative_path(token)
    })
}

fn is_relative_path(token: &str) -> bool {
    !token.contains("://") && token.split('/').filter(|part| !part.is_empty()).count() >= 2
}

fn is_atx_heading(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let hashes = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    (1..=6).contains(&hashes) && matches!(trimmed.as_bytes().get(hashes), None | Some(b' ' | b'\t'))
}

fn is_setext_underline(line: &str) -> bool {
    let compact = line.trim_matches([' ', '\t']);
    !compact.is_empty()
        && compact.chars().next().is_some_and(|marker| {
            matches!(marker, '=' | '-') && compact.chars().all(|character| character == marker)
        })
}

fn looks_like_filename_list(content: &str, lines: &[LogicalLine]) -> bool {
    lines.len() >= 2
        && lines.iter().all(|line| {
            let value = content[line.start..line.content_end].trim_matches([' ', '\t']);
            !value.is_empty() && !value.chars().any(char::is_whitespace) && is_bare_filename(value)
        })
}

fn is_bare_filename(value: &str) -> bool {
    if let Some(stripped) = value.strip_prefix('.') {
        return !stripped.is_empty()
            && stripped.chars().all(|character| {
                character.is_alphanumeric() || matches!(character, '-' | '_' | '.')
            });
    }
    let Some((stem, extension)) = value.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && !extension.is_empty()
        && extension.len() <= 12
        && stem
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_' | '.'))
        && extension.chars().all(char::is_alphanumeric)
}

fn is_windows_path(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn ranges_intersect(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
