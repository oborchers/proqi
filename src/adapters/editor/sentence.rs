//! Unicode sentence ranges with Proqi's paragraph-preserving profile.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

#[derive(Clone, Debug, Eq, PartialEq)]
struct SentenceUnit {
    owned: Range<usize>,
    deletion: Range<usize>,
}

pub(super) fn deletion_ranges(
    text: &str,
    cursor: usize,
    selection: Option<(usize, usize)>,
) -> Vec<Range<usize>> {
    let units = sentence_units(text);
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
            .map(|unit| unit.deletion.clone())
            .collect(),
    )
}

fn sentence_units(text: &str) -> Vec<SentenceUnit> {
    paragraph_ranges(text)
        .into_iter()
        .flat_map(|paragraph| units_in_paragraph(text, paragraph))
        .collect()
}

fn units_in_paragraph(text: &str, paragraph: Range<usize>) -> Vec<SentenceUnit> {
    let source = &text[paragraph.clone()];
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
                paragraph.start + offset + core.start..paragraph.start + offset + core.end
            })
        })
        .collect::<Vec<_>>();
    cores
        .iter()
        .enumerate()
        .map(|(index, core)| {
            let owned_start = if index == 0 {
                paragraph.start
            } else {
                core.start
            };
            let owned_end = cores
                .get(index + 1)
                .map_or(paragraph.end, |next| next.start);
            let deletion_start = if index + 1 == cores.len() && index > 0 {
                cores[index - 1].end
            } else {
                owned_start
            };
            SentenceUnit {
                owned: owned_start..owned_end,
                deletion: deletion_start..owned_end,
            }
        })
        .collect()
}

fn non_whitespace_bounds(text: &str) -> Option<Range<usize>> {
    let start = text
        .char_indices()
        .find_map(|(index, character)| (!character.is_whitespace()).then_some(index))?;
    let (last, character) = text
        .char_indices()
        .rev()
        .find(|(_, character)| !character.is_whitespace())?;
    Some(start..last + character.len_utf8())
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
