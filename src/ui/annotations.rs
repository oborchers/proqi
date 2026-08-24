//! Exact paste payloads and folded board presentation.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::domain::{ContentAnnotation, ContentAnnotationKind};

const LARGE_PASTE_LINES: usize = 12;
const LARGE_PASTE_GRAPHEMES: usize = 1_200;

/// Exact inserted text with optional durable presentation provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PastePayload {
    /// Canonical text supplied to editing, copy, export, and submission.
    pub content: String,
    /// Presentation metadata over UTF-8 byte ranges in `content`.
    pub annotations: Vec<ContentAnnotation>,
}

impl PastePayload {
    /// Preserve plain text and fold it only when it exceeds the context threshold.
    #[must_use]
    pub fn text(content: String) -> Self {
        let lines = content.lines().count().max(1);
        let graphemes = content.graphemes(true).count();
        let annotations = if !content.is_empty()
            && (lines >= LARGE_PASTE_LINES || graphemes >= LARGE_PASTE_GRAPHEMES)
        {
            vec![ContentAnnotation {
                start: 0,
                end: content.len(),
                kind: ContentAnnotationKind::LargePaste { lines, graphemes },
            }]
        } else {
            Vec::new()
        };
        Self {
            content,
            annotations,
        }
    }

    /// Construct a payload whose annotations were derived by a trusted adapter.
    #[must_use]
    pub const fn annotated(content: String, annotations: Vec<ContentAnnotation>) -> Self {
        Self {
            content,
            annotations,
        }
    }
}

pub(super) fn presentation(content: &str, annotations: &[ContentAnnotation]) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    let mut image = 0;
    let mut file = 0;
    for annotation in annotations {
        let Some(prefix) = content.get(cursor..annotation.start) else {
            return content.to_owned();
        };
        output.push_str(prefix);
        match &annotation.kind {
            ContentAnnotationKind::Attachment {
                image: is_image,
                display_name,
            } => {
                let (kind, number) = if *is_image {
                    image += 1;
                    ("Image", image)
                } else {
                    file += 1;
                    ("File", file)
                };
                output.push('[');
                output.push_str(kind);
                output.push(' ');
                output.push_str(&number.to_string());
                output.push_str("]  ");
                output.push_str(display_name);
            }
            ContentAnnotationKind::LargePaste { lines, graphemes } => {
                output.push_str("[Pasted text]  ");
                output.push_str(&lines.to_string());
                output.push_str(" lines · ");
                output.push_str(&grouped(*graphemes));
                output.push_str(" characters");
            }
        }
        cursor = annotation.end;
    }
    match content.get(cursor..) {
        Some(suffix) => output.push_str(suffix),
        None => return content.to_owned(),
    }
    output
}

pub(super) fn rebase(
    before: &str,
    after: &str,
    annotations: &[ContentAnnotation],
    inserted: &[ContentAnnotation],
) -> Vec<ContentAnnotation> {
    let (before_start, before_end, after_start, after_end) = changed_span(before, after);
    let removed = before_end.saturating_sub(before_start);
    let added = after_end.saturating_sub(after_start);
    let mut rebased = annotations
        .iter()
        .filter_map(|annotation| {
            if annotation.end <= before_start {
                Some(annotation.clone())
            } else if annotation.start >= before_end {
                Some(shift(annotation, added, removed))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    rebased.extend(inserted.iter().map(|annotation| ContentAnnotation {
        start: after_start.saturating_add(annotation.start),
        end: after_start.saturating_add(annotation.end),
        kind: annotation.kind.clone(),
    }));
    rebased.sort_by_key(|annotation| annotation.start);
    rebased
}

fn shift(annotation: &ContentAnnotation, added: usize, removed: usize) -> ContentAnnotation {
    ContentAnnotation {
        start: annotation
            .start
            .saturating_add(added)
            .saturating_sub(removed),
        end: annotation.end.saturating_add(added).saturating_sub(removed),
        kind: annotation.kind.clone(),
    }
}

fn changed_span(before: &str, after: &str) -> (usize, usize, usize, usize) {
    let prefix = before
        .char_indices()
        .zip(after.char_indices())
        .take_while(|((_, left), (_, right))| left == right)
        .last()
        .map_or(0, |((offset, value), _)| offset + value.len_utf8());
    let before_tail = &before[prefix..];
    let after_tail = &after[prefix..];
    let suffix = before_tail
        .chars()
        .rev()
        .zip(after_tail.chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    (
        prefix,
        before.len().saturating_sub(suffix),
        prefix,
        after.len().saturating_sub(suffix),
    )
}

fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}
