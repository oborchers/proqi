//! Exact paste payloads and folded board presentation.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::domain::{ContentAnnotation, ContentAnnotationKind};
use crate::ports::editor::{OffsetAffinity, TextChange, TextChangeSet};

const LARGE_PASTE_LINES: usize = 12;
const LARGE_PASTE_GRAPHEMES: usize = 1_200;
const INACCESSIBLE_SUFFIX: &str = " [inaccessible]";

/// Exact inserted text with optional durable presentation provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PastePayload {
    /// Canonical text supplied to editing, copy, export, and submission.
    pub content: String,
    /// Presentation metadata over UTF-8 byte ranges in `content`.
    pub annotations: Vec<ContentAnnotation>,
    verified_paths: Vec<String>,
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
            verified_paths: Vec::new(),
        }
    }

    /// Construct a payload whose annotations were derived by a trusted adapter.
    #[must_use]
    pub const fn annotated(content: String, annotations: Vec<ContentAnnotation>) -> Self {
        Self {
            content,
            annotations,
            verified_paths: Vec::new(),
        }
    }

    /// Retain transient accessibility evidence established by the producing adapter.
    #[must_use]
    pub(crate) fn with_verified_attachments(mut self) -> Self {
        self.verified_paths = self
            .annotations
            .iter()
            .filter_map(|annotation| {
                matches!(annotation.kind, ContentAnnotationKind::Attachment { .. })
                    .then(|| self.content.get(annotation.start..annotation.end))
                    .flatten()
                    .map(ToOwned::to_owned)
            })
            .collect();
        self
    }

    pub(in crate::ui) fn into_parts(self) -> (String, Vec<ContentAnnotation>, Vec<String>) {
        (self.content, self.annotations, self.verified_paths)
    }
}

/// One folded range in projected presentation text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PresentedFold {
    pub(super) annotation_index: usize,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) content_end: usize,
    pub(super) canonical_start: usize,
    pub(super) canonical_end: usize,
    pub(super) collapsed: bool,
    pub(super) inaccessible: bool,
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
    project_with_health(content, annotations, expanded, |_| false)
}

pub(super) fn project_with_health(
    content: &str,
    annotations: &[ContentAnnotation],
    expanded: &[usize],
    mut inaccessible: impl FnMut(usize) -> bool,
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
        let inaccessible = matches!(annotation.kind, ContentAnnotationKind::Attachment { .. })
            && inaccessible(annotation_index);
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
            let content_end = output.len();
            if inaccessible {
                output.push_str(INACCESSIBLE_SUFFIX);
            }
            folds.push(fold(
                annotation_index,
                start,
                output.len(),
                content_end,
                annotation,
                false,
                inaccessible,
            ));
            cursor = annotation.end;
            continue;
        }
        push_collapsed_label(
            &mut output,
            &annotation.kind,
            &mut image,
            &mut file,
            inaccessible,
        );
        folds.push(fold(
            annotation_index,
            start,
            output.len(),
            output.len(),
            annotation,
            true,
            inaccessible,
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

fn push_collapsed_label(
    output: &mut String,
    kind: &ContentAnnotationKind,
    image_count: &mut usize,
    file_count: &mut usize,
    inaccessible: bool,
) {
    match kind {
        ContentAnnotationKind::Attachment { image, .. } => {
            let (label, number) = if *image {
                *image_count += 1;
                ("Image", *image_count)
            } else {
                *file_count += 1;
                ("File", *file_count)
            };
            output.push('[');
            output.push_str(label);
            output.push(' ');
            output.push_str(&number.to_string());
            if inaccessible {
                output.push_str(" · inaccessible");
            }
            output.push(']');
        }
        ContentAnnotationKind::LargePaste { lines, graphemes } => {
            output.push_str("[Pasted text · ");
            output.push_str(&lines.to_string());
            output.push_str(" lines · ");
            output.push_str(&grouped(*graphemes));
            output.push_str(" characters]");
        }
        ContentAnnotationKind::InvocationReference { display_name } => {
            output.push_str(display_name);
        }
    }
}

fn fold(
    annotation_index: usize,
    start: usize,
    end: usize,
    content_end: usize,
    annotation: &ContentAnnotation,
    collapsed: bool,
    inaccessible: bool,
) -> PresentedFold {
    PresentedFold {
        annotation_index,
        start,
        end,
        content_end,
        canonical_start: annotation.start,
        canonical_end: annotation.end,
        collapsed,
        inaccessible,
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
    let mut rebased = annotations
        .iter()
        .filter(|annotation| {
            !changes
                .as_slice()
                .iter()
                .any(|change| intersects(annotation, change))
        })
        .filter_map(|annotation| map_annotation(before, changes, annotation))
        .collect::<Vec<_>>();
    if let [change] = changes.as_slice() {
        let new_range = change.new_range();
        rebased.extend(inserted.iter().filter_map(|annotation| {
            let start = new_range.start.checked_add(annotation.start)?;
            let end = new_range.start.checked_add(annotation.end)?;
            (end <= new_range.end && after.get(start..end).is_some()).then(|| ContentAnnotation {
                start,
                end,
                kind: annotation.kind.clone(),
            })
        }));
    }
    rebased.sort_by_key(|annotation| annotation.start);
    rebased
}

fn intersects(annotation: &ContentAnnotation, change: &TextChange) -> bool {
    let old = change.old_range();
    if old.is_empty() {
        annotation.start < old.start && old.start < annotation.end
    } else {
        old.start < annotation.end && annotation.start < old.end
    }
}

fn map_annotation(
    before: &str,
    changes: &TextChangeSet,
    annotation: &ContentAnnotation,
) -> Option<ContentAnnotation> {
    let start = changes
        .map_old_offset(before, annotation.start, OffsetAffinity::After)
        .ok()?;
    let end = changes
        .map_old_offset(before, annotation.end, OffsetAffinity::Before)
        .ok()?;
    (start < end).then(|| ContentAnnotation {
        start,
        end,
        kind: annotation.kind.clone(),
    })
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
