//! Exact paste payloads and folded board presentation.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::domain::{
    AnnotationTextChange, ContentAnnotation, ContentAnnotationKind, rebase_annotations,
};
use crate::ports::editor::TextChangeSet;

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

/// One folded range in projected presentation text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PresentedFold {
    pub(super) annotation_index: usize,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) canonical_start: usize,
    pub(super) canonical_end: usize,
    pub(super) collapsed: bool,
}

/// Display-only projection that never replaces canonical content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Presentation {
    pub(super) content: String,
    pub(super) folds: Vec<PresentedFold>,
}

pub(super) fn project(
    content: &str,
    annotations: &[ContentAnnotation],
    expanded: &[usize],
) -> Presentation {
    let mut output = String::new();
    let mut folds = Vec::new();
    let mut cursor = 0;
    let mut image = 0;
    let mut file = 0;
    for (annotation_index, annotation) in annotations.iter().enumerate() {
        let Some(prefix) = content.get(cursor..annotation.start) else {
            return plain(content);
        };
        output.push_str(prefix);
        let start = output.len();
        if expanded.contains(&annotation_index) {
            if let ContentAnnotationKind::Attachment { image: true, .. } = &annotation.kind {
                image += 1;
            } else if matches!(
                &annotation.kind,
                ContentAnnotationKind::Attachment { image: false, .. }
            ) {
                file += 1;
            }
            let Some(exact) = content.get(annotation.start..annotation.end) else {
                return plain(content);
            };
            output.push_str(exact);
            folds.push(fold(
                annotation_index,
                start,
                output.len(),
                annotation,
                false,
            ));
            cursor = annotation.end;
            continue;
        }
        match &annotation.kind {
            ContentAnnotationKind::Attachment {
                image: is_image, ..
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
                output.push(']');
            }
            ContentAnnotationKind::LargePaste { lines, graphemes } => {
                output.push_str("[Pasted text · ");
                output.push_str(&lines.to_string());
                output.push_str(" lines · ");
                output.push_str(&grouped(*graphemes));
                output.push_str(" characters]");
            }
        }
        folds.push(fold(
            annotation_index,
            start,
            output.len(),
            annotation,
            true,
        ));
        cursor = annotation.end;
    }
    match content.get(cursor..) {
        Some(suffix) => output.push_str(suffix),
        None => return plain(content),
    }
    Presentation {
        content: output,
        folds,
    }
}

fn fold(
    annotation_index: usize,
    start: usize,
    end: usize,
    annotation: &ContentAnnotation,
    collapsed: bool,
) -> PresentedFold {
    PresentedFold {
        annotation_index,
        start,
        end,
        canonical_start: annotation.start,
        canonical_end: annotation.end,
        collapsed,
    }
}

fn plain(content: &str) -> Presentation {
    Presentation {
        content: content.to_owned(),
        folds: Vec::new(),
    }
}

pub(super) fn rebase(
    before: &str,
    after: &str,
    changes: &TextChangeSet,
    annotations: &[ContentAnnotation],
    inserted: &[ContentAnnotation],
) -> Vec<ContentAnnotation> {
    let changes = changes
        .as_slice()
        .iter()
        .map(|change| AnnotationTextChange {
            old: change.old_range(),
            new: change.new_range(),
        })
        .collect::<Vec<_>>();
    annotations_or_empty(rebase_annotations(
        before,
        after,
        &changes,
        annotations,
        inserted,
    ))
}

#[expect(
    clippy::manual_unwrap_or_default,
    reason = "invalid display metadata deliberately degrades to plain canonical text"
)]
fn annotations_or_empty(
    result: Result<Vec<ContentAnnotation>, crate::domain::DomainError>,
) -> Vec<ContentAnnotation> {
    match result {
        Ok(annotations) => annotations,
        Err(_) => Vec::new(),
    }
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

#[cfg(test)]
mod tests;
