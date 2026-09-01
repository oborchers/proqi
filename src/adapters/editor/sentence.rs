//! Unicode sentence ranges with Proqi's paragraph-preserving profile.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

use crate::ports::text_layout::{LogicalLine, logical_lines};

#[derive(Clone, Debug, Eq, PartialEq)]
struct SentenceUnit {
    owned: Range<usize>,
    deletions: Vec<Range<usize>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SentenceBlock {
    owned_start: usize,
    content: Range<usize>,
    leading_deletion: Option<Range<usize>>,
}

pub(super) fn deletion_ranges(
    text: &str,
    cursor: usize,
    selection: Option<(usize, usize)>,
    list_indent_width: u8,
) -> Vec<Range<usize>> {
    let units = sentence_units(text, list_indent_width);
    let targets: Vec<&SentenceUnit> = selection.map_or_else(
        || {
            sentence_at_cursor(&units, cursor.min(text.len()))
                .into_iter()
                .collect()
        },
        |(start, end)| {
            units
                .iter()
                .filter(|unit| start < unit.owned.end && unit.owned.start < end)
                .collect()
        },
    );
    merge_ranges(
        targets
            .into_iter()
            .flat_map(|unit| unit.deletions.iter().cloned())
            .collect(),
    )
}

fn sentence_units(text: &str, list_indent_width: u8) -> Vec<SentenceUnit> {
    let lines = logical_lines(text);
    let list_prefixes =
        super::smart_lists::recognized_prefix_lengths(text, &lines, list_indent_width);
    paragraph_ranges(text)
        .into_iter()
        .flat_map(|paragraph| blocks_in_paragraph(text, paragraph, &lines, &list_prefixes))
        .flat_map(|block| units_in_block(text, &block))
        .collect()
}

fn units_in_block(text: &str, block: &SentenceBlock) -> Vec<SentenceUnit> {
    let source = &text[block.content.clone()];
    let shadow = source
        .chars()
        .map(|character| match character {
            '\r' | '\n' => ' ',
            other => other,
        })
        .collect::<String>();
    let cores = shadow
        .split_sentence_bound_indices()
        .filter_map(|(offset, segment)| {
            let original = &source[offset..offset + segment.len()];
            non_whitespace_bounds(original).map(|core| {
                block.content.start + offset + core.start..block.content.start + offset + core.end
            })
        })
        .collect::<Vec<_>>();
    if cores.is_empty() {
        return vec![SentenceUnit {
            owned: block.owned_start..block.content.end,
            deletions: Vec::new(),
        }];
    }
    cores
        .iter()
        .enumerate()
        .map(|(index, core)| {
            let owned_start = if index == 0 {
                block.owned_start
            } else {
                core.start
            };
            let owned_end = cores
                .get(index + 1)
                .map_or(block.content.end, |next| next.start);
            let deletion_start = if index + 1 == cores.len() && index > 0 {
                cores[index - 1].end
            } else if index == 0 {
                block.content.start
            } else {
                owned_start
            };
            let mut deletions = Vec::with_capacity(2);
            if index == 0
                && let Some(leading) = block.leading_deletion.clone()
                && !leading.is_empty()
            {
                deletions.push(leading);
            }
            deletions.push(deletion_start..owned_end);
            SentenceUnit {
                owned: owned_start..owned_end,
                deletions,
            }
        })
        .collect()
}

fn blocks_in_paragraph(
    text: &str,
    paragraph: Range<usize>,
    lines: &[LogicalLine],
    list_prefixes: &[Option<usize>],
) -> Vec<SentenceBlock> {
    let first_line = lines.partition_point(|line| line.start < paragraph.start);
    let after_last_line = lines.partition_point(|line| line.start < paragraph.end);
    let item_lines = (first_line..after_last_line)
        .filter_map(|index| list_prefixes[index].map(|prefix_len| (index, prefix_len)))
        .collect::<Vec<_>>();
    let Some((first_line, _)) = item_lines.first().copied() else {
        return vec![SentenceBlock {
            owned_start: paragraph.start,
            content: paragraph,
            leading_deletion: None,
        }];
    };
    let mut blocks = Vec::new();
    let first_item_start = lines[first_line].start;
    let whitespace_prelude = paragraph.start < first_item_start
        && text[paragraph.start..first_item_start]
            .chars()
            .all(char::is_whitespace);
    if paragraph.start < first_item_start && !whitespace_prelude {
        let content_end = line_before(lines, first_line).content_end;
        blocks.push(SentenceBlock {
            owned_start: paragraph.start,
            content: paragraph.start..content_end,
            leading_deletion: None,
        });
    }
    for (position, (line_index, prefix_len)) in item_lines.iter().copied().enumerate() {
        let line = lines[line_index];
        let content_end = item_lines
            .get(position + 1)
            .map_or(paragraph.end, |(next, _)| {
                line_before(lines, *next).content_end
            });
        blocks.push(SentenceBlock {
            owned_start: if position == 0 && whitespace_prelude {
                paragraph.start
            } else {
                line.start
            },
            content: line.start + prefix_len..content_end,
            leading_deletion: (position == 0 && whitespace_prelude)
                .then_some(paragraph.start..line.start),
        });
    }
    blocks
}

fn line_before(lines: &[LogicalLine], index: usize) -> LogicalLine {
    lines[index.saturating_sub(1)]
}

fn non_whitespace_bounds(text: &str) -> Option<Range<usize>> {
    let start = text.grapheme_indices(true).find_map(|(index, grapheme)| {
        (!grapheme_starts_with_whitespace(grapheme)).then_some(index)
    })?;
    let (last, grapheme) = text
        .grapheme_indices(true)
        .rev()
        .find(|(_, grapheme)| !grapheme_starts_with_whitespace(grapheme))?;
    Some(start..last + grapheme.len())
}

fn grapheme_starts_with_whitespace(grapheme: &str) -> bool {
    grapheme.chars().next().is_some_and(char::is_whitespace)
}

fn paragraph_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for boundary in blank_line_boundaries(text) {
        if start < boundary.start {
            ranges.push(start..boundary.start);
        }
        start = boundary.end;
    }
    if start < text.len() {
        ranges.push(start..text.len());
    }
    ranges
}

fn blank_line_boundaries(text: &str) -> Vec<Range<usize>> {
    let mut boundaries = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let Some(first_end) = newline_end(text, cursor) else {
            cursor += 1;
            continue;
        };
        let Some(mut end) = following_newline(text, first_end) else {
            cursor = first_end;
            continue;
        };
        while let Some(next) = following_newline(text, end) {
            end = next;
        }
        boundaries.push(cursor..end);
        cursor = end;
    }
    boundaries
}

fn following_newline(text: &str, mut cursor: usize) -> Option<usize> {
    while cursor < text.len() {
        if let Some(end) = newline_end(text, cursor) {
            return Some(end);
        }
        let character = text[cursor..].chars().next()?;
        if !character.is_whitespace() {
            return None;
        }
        cursor += character.len_utf8();
    }
    None
}

fn newline_end(text: &str, cursor: usize) -> Option<usize> {
    match text.as_bytes().get(cursor..)? {
        [b'\r', b'\n', ..] => Some(cursor + 2),
        [b'\r' | b'\n', ..] => Some(cursor + 1),
        _ => None,
    }
}

fn sentence_at_cursor(units: &[SentenceUnit], cursor: usize) -> Option<&SentenceUnit> {
    units
        .iter()
        .find(|unit| unit.owned.start <= cursor && cursor < unit.owned.end)
        .or_else(|| units.iter().rev().find(|unit| unit.owned.end <= cursor))
        .or_else(|| units.first())
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(previous) if range.start <= previous.end => {
                previous.end = previous.end.max(range.end);
            }
            _ => merged.push(range),
        }
    }
    merged
}
