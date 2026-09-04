//! Annotation-safe orchestration for explicit paste reflow.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::{
    domain::{ContentAnnotation, ContentAnnotationKind},
    ports::editor::{OffsetAffinity, TextChange},
};

use super::{LARGE_PASTE_GRAPHEMES, LARGE_PASTE_LINES, PastePayload};

#[cfg(test)]
mod tests;

pub(in crate::ui) enum PasteReflow {
    Changed(PastePayload),
    Unchanged,
    Empty,
}

impl PastePayload {
    pub(in crate::ui) fn reflow(&self) -> Result<PasteReflow, ()> {
        crate::domain::validate_annotations(&self.content, &self.annotations).map_err(|_| ())?;
        let protected = self
            .annotations
            .iter()
            .filter(|annotation| !is_large(annotation))
            .map(|annotation| annotation.start..annotation.end)
            .collect::<Vec<_>>();
        let isolated = self
            .annotations
            .iter()
            .filter(|annotation| is_large(annotation))
            .map(|annotation| annotation.start..annotation.end)
            .collect::<Vec<_>>();
        let transformed =
            crate::ui::paste_reflow::reflow_text_isolated(&self.content, &protected, &isolated)
                .map_err(|_| ())?;
        if transformed.content.is_empty() {
            return Ok(PasteReflow::Empty);
        }
        if transformed.changes.is_empty() {
            return Ok(PasteReflow::Unchanged);
        }
        let annotations = reflow_annotations(self, &transformed).map_err(|_| ())?;
        Ok(PasteReflow::Changed(Self {
            content: transformed.content,
            annotations,
            verified_paths: self.verified_paths.clone(),
            preserve_owned_annotations: self.preserve_owned_annotations,
        }))
    }
}

fn reflow_annotations(
    payload: &PastePayload,
    transformed: &crate::ui::paste_reflow::ReflowedText,
) -> Result<Vec<ContentAnnotation>, crate::domain::DomainError> {
    let mut mapper = OffsetMapper::new(transformed.changes.as_slice());
    let mut annotations = Vec::with_capacity(payload.annotations.len());
    let mut isolated = transformed.isolated.iter();
    for annotation in &payload.annotations {
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
                annotations.push(annotation);
            }
        } else {
            let before = payload.content.get(annotation.start..annotation.end);
            let after = transformed.content.get(start..end);
            if before.is_none() || before != after {
                return Err(crate::domain::DomainError::InvalidContentAnnotation);
            }
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
    Ok(annotations)
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

pub(super) fn large_paste_annotation(
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
