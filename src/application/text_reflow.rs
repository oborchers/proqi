//! Annotation-safe canonical policy for explicit whole-text spacing cleanup.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::{OffsetAffinity, TextChange, TextChangeSet},
};

pub(crate) mod format;
#[cfg(test)]
mod tests;

const LARGE_PASTE_LINES: usize = 12;
const LARGE_PASTE_GRAPHEMES: usize = 1_200;

pub(crate) enum TextReflowOutcome {
    Changed {
        content: String,
        annotations: Vec<ContentAnnotation>,
    },
    Unchanged,
    Empty,
}

pub(crate) struct TextReflowProjection {
    pub(crate) outcome: TextReflowOutcome,
    pub(crate) changes: TextChangeSet,
    pub(crate) annotation_origins: Vec<(usize, usize)>,
}

pub(crate) struct ReflowedAnnotations {
    pub(crate) annotations: Vec<ContentAnnotation>,
    pub(crate) origins: Vec<(usize, usize)>,
}

pub(crate) fn reflow(
    content: &str,
    annotations: &[ContentAnnotation],
) -> Result<TextReflowProjection, ()> {
    crate::domain::validate_annotations(content, annotations).map_err(|_| ())?;
    let protected = annotations
        .iter()
        .filter(|annotation| !is_large(annotation))
        .map(|annotation| annotation.start..annotation.end)
        .collect::<Vec<_>>();
    let isolated = annotations
        .iter()
        .filter(|annotation| is_large(annotation))
        .map(|annotation| annotation.start..annotation.end)
        .collect::<Vec<_>>();
    let transformed =
        format::reflow_text_isolated(content, &protected, &isolated).map_err(|_| ())?;
    if transformed.content.is_empty() {
        return Ok(TextReflowProjection {
            outcome: TextReflowOutcome::Empty,
            changes: transformed.changes,
            annotation_origins: Vec::new(),
        });
    }
    let ReflowedAnnotations {
        annotations: transformed_annotations,
        origins,
    } = reflow_annotations(content, annotations, &transformed).map_err(|_| ())?;
    if transformed.changes.is_empty() && transformed_annotations == annotations {
        return Ok(TextReflowProjection {
            outcome: TextReflowOutcome::Unchanged,
            changes: transformed.changes,
            annotation_origins: origins,
        });
    }
    Ok(TextReflowProjection {
        outcome: TextReflowOutcome::Changed {
            content: transformed.content,
            annotations: transformed_annotations,
        },
        changes: transformed.changes,
        annotation_origins: origins,
    })
}

pub(crate) fn reflow_annotations(
    content: &str,
    source_annotations: &[ContentAnnotation],
    transformed: &format::ReflowedText,
) -> Result<ReflowedAnnotations, crate::domain::DomainError> {
    let mut mapper = OffsetMapper::new(transformed.changes.as_slice());
    let mut annotations = Vec::with_capacity(source_annotations.len());
    let mut origins = Vec::with_capacity(source_annotations.len());
    let mut isolated = transformed.isolated.iter();
    for (index, annotation) in source_annotations.iter().enumerate() {
        let large = is_large(annotation);
        let (start, end) = if large {
            let (old, new) = isolated
                .next()
                .ok_or(crate::domain::DomainError::InvalidContentAnnotation)?;
            if *old != (annotation.start..annotation.end) {
                return Err(crate::domain::DomainError::InvalidContentAnnotation);
            }
            (new.start, new.end)
        } else {
            let start = mapper
                .map(annotation.start, OffsetAffinity::After)
                .ok_or(crate::domain::DomainError::InvalidContentAnnotation)?;
            let end = mapper
                .map(annotation.end, OffsetAffinity::Before)
                .ok_or(crate::domain::DomainError::InvalidContentAnnotation)?;
            (start, end)
        };
        if large {
            if let Some(annotation) = large_paste_annotation(&transformed.content, start, end) {
                origins.push((index, annotations.len()));
                annotations.push(annotation);
            }
        } else {
            let before = content.get(annotation.start..annotation.end);
            let after = transformed.content.get(start..end);
            if before.is_none() || before != after {
                return Err(crate::domain::DomainError::InvalidContentAnnotation);
            }
            origins.push((index, annotations.len()));
            annotations.push(ContentAnnotation {
                start,
                end,
                kind: annotation.kind.clone(),
            });
        }
    }
    if isolated.next().is_some() {
        return Err(crate::domain::DomainError::InvalidContentAnnotation);
    }
    crate::domain::validate_annotations(&transformed.content, &annotations)?;
    Ok(ReflowedAnnotations {
        annotations,
        origins,
    })
}

struct OffsetMapper<'a> {
    changes: &'a [TextChange],
    index: usize,
    old_cursor: usize,
    new_cursor: usize,
}

impl<'a> OffsetMapper<'a> {
    const fn new(changes: &'a [TextChange]) -> Self {
        Self {
            changes,
            index: 0,
            old_cursor: 0,
            new_cursor: 0,
        }
    }

    fn map(&mut self, offset: usize, affinity: OffsetAffinity) -> Option<usize> {
        while let Some(change) = self.changes.get(self.index) {
            let old = change.old_range();
            let new = change.new_range();
            if offset < old.start {
                return self
                    .new_cursor
                    .checked_add(offset.checked_sub(self.old_cursor)?);
            }
            if old.is_empty() && offset == old.start {
                return Some(match affinity {
                    OffsetAffinity::Before => new.start,
                    OffsetAffinity::After => new.end,
                });
            }
            if offset < old.end {
                return Some(match affinity {
                    OffsetAffinity::Before => new.start,
                    OffsetAffinity::After => new.end,
                });
            }
            self.old_cursor = old.end;
            self.new_cursor = new.end;
            self.index += 1;
        }
        self.new_cursor
            .checked_add(offset.checked_sub(self.old_cursor)?)
    }
}

fn is_large(annotation: &ContentAnnotation) -> bool {
    matches!(annotation.kind, ContentAnnotationKind::LargePaste { .. })
}

pub(crate) fn large_paste_annotation(
    content: &str,
    start: usize,
    end: usize,
) -> Option<ContentAnnotation> {
    let value = content.get(start..end)?;
    let lines = value.lines().count().max(1);
    let graphemes = value.graphemes(true).count();
    (!value.is_empty() && (lines >= LARGE_PASTE_LINES || graphemes >= LARGE_PASTE_GRAPHEMES))
        .then_some(ContentAnnotation {
            start,
            end,
            kind: ContentAnnotationKind::LargePaste { lines, graphemes },
        })
}

pub(crate) use format::position_changes;
#[cfg(test)]
pub(crate) use format::reflow_text_isolated;
