//! Canonical range transformations for durable content annotations.

use std::ops::Range;

use super::{ContentAnnotation, DomainError, validate_annotations};

/// One validated replacement expressed in before and after byte coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnotationTextChange {
    /// Replaced range in the original content.
    pub old: Range<usize>,
    /// Resulting range in the new content.
    pub new: Range<usize>,
}

/// Partition annotations at one exact content boundary.
///
/// Complete annotations retain identity on exactly one side. Any annotation
/// crossing the boundary dissolves instead of claiming fragment provenance.
///
/// # Errors
///
/// Returns a domain error for invalid annotations or a non-boundary offset.
pub fn partition_annotations(
    content: &str,
    annotations: &[ContentAnnotation],
    at: usize,
) -> Result<(Vec<ContentAnnotation>, Vec<ContentAnnotation>), DomainError> {
    validate_range(content, annotations, at..at)?;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for annotation in annotations {
        if annotation.end <= at {
            left.push(annotation.clone());
        } else if annotation.start >= at {
            right.push(ContentAnnotation {
                start: annotation.start - at,
                end: annotation.end - at,
                kind: annotation.kind.clone(),
            });
        }
    }
    Ok((left, right))
}

/// Partition annotations between content remaining after a closure and exact
/// extracted content.
///
/// # Errors
///
/// Returns a domain error for invalid annotations, an empty range, or invalid
/// UTF-8 boundaries.
pub fn extract_annotations(
    content: &str,
    annotations: &[ContentAnnotation],
    range: Range<usize>,
) -> Result<(Vec<ContentAnnotation>, Vec<ContentAnnotation>), DomainError> {
    validate_range(content, annotations, range.clone())?;
    if range.is_empty() {
        return Err(DomainError::EmptyContentRange);
    }
    let removed = range.end - range.start;
    let mut remaining = Vec::new();
    let mut extracted = Vec::new();
    for annotation in annotations {
        if annotation.end <= range.start {
            remaining.push(annotation.clone());
        } else if annotation.start >= range.end {
            remaining.push(ContentAnnotation {
                start: annotation.start - removed,
                end: annotation.end - removed,
                kind: annotation.kind.clone(),
            });
        } else if annotation.start >= range.start && annotation.end <= range.end {
            extracted.push(ContentAnnotation {
                start: annotation.start - range.start,
                end: annotation.end - range.start,
                kind: annotation.kind.clone(),
            });
        }
    }
    Ok((remaining, extracted))
}

/// Shift complete annotation sets into concatenated content in source order.
///
/// # Errors
///
/// Returns a domain error when a source annotation set is invalid or an offset
/// overflows.
pub fn merge_annotations<'a>(
    parts: impl IntoIterator<Item = (&'a str, &'a [ContentAnnotation])>,
    separator: &str,
) -> Result<Vec<ContentAnnotation>, DomainError> {
    let parts = parts.into_iter().collect::<Vec<_>>();
    let mut merged = Vec::new();
    let mut offset = 0usize;
    for (index, (content, annotations)) in parts.iter().enumerate() {
        validate_annotations(content, annotations)?;
        for annotation in *annotations {
            merged.push(ContentAnnotation {
                start: offset
                    .checked_add(annotation.start)
                    .ok_or(DomainError::ContentLengthOverflow)?,
                end: offset
                    .checked_add(annotation.end)
                    .ok_or(DomainError::ContentLengthOverflow)?,
                kind: annotation.kind.clone(),
            });
        }
        offset = offset
            .checked_add(content.len())
            .ok_or(DomainError::ContentLengthOverflow)?;
        if index + 1 < parts.len() {
            offset = offset
                .checked_add(separator.len())
                .ok_or(DomainError::ContentLengthOverflow)?;
        }
    }
    Ok(merged)
}

/// Rebase durable annotations through an ordered exact text transaction.
///
/// Existing annotations intersected by replacement are removed. Unaffected
/// annotations keep their provenance and use explicit edge affinity. Optional
/// inserted annotations are relative to the single inserted replacement.
///
/// # Errors
///
/// Returns a domain error for invalid annotation or change ranges.
pub fn rebase_annotations(
    before: &str,
    after: &str,
    changes: &[AnnotationTextChange],
    annotations: &[ContentAnnotation],
    inserted: &[ContentAnnotation],
) -> Result<Vec<ContentAnnotation>, DomainError> {
    validate_annotations(before, annotations)?;
    validate_changes(before, after, changes)?;
    let mut rebased = annotations
        .iter()
        .filter(|annotation| {
            !changes
                .iter()
                .any(|change| intersects(annotation, &change.old))
        })
        .map(|annotation| {
            Ok(ContentAnnotation {
                start: map_offset(annotation.start, changes, Affinity::After)?,
                end: map_offset(annotation.end, changes, Affinity::Before)?,
                kind: annotation.kind.clone(),
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    rebased.retain(|annotation| annotation.start < annotation.end);
    if let [change] = changes {
        validate_annotations(
            after
                .get(change.new.clone())
                .ok_or(DomainError::InvalidContentRange)?,
            inserted,
        )?;
        for annotation in inserted {
            rebased.push(ContentAnnotation {
                start: change
                    .new
                    .start
                    .checked_add(annotation.start)
                    .ok_or(DomainError::ContentLengthOverflow)?,
                end: change
                    .new
                    .start
                    .checked_add(annotation.end)
                    .ok_or(DomainError::ContentLengthOverflow)?,
                kind: annotation.kind.clone(),
            });
        }
    } else if !inserted.is_empty() {
        return Err(DomainError::InvalidContentAnnotation);
    }
    rebased.sort_by_key(|annotation| annotation.start);
    validate_annotations(after, &rebased)?;
    Ok(rebased)
}

fn validate_range(
    content: &str,
    annotations: &[ContentAnnotation],
    range: Range<usize>,
) -> Result<(), DomainError> {
    validate_annotations(content, annotations)?;
    if range.start > range.end
        || range.end > content.len()
        || !content.is_char_boundary(range.start)
        || !content.is_char_boundary(range.end)
    {
        return Err(DomainError::InvalidContentRange);
    }
    Ok(())
}

fn validate_changes(
    before: &str,
    after: &str,
    changes: &[AnnotationTextChange],
) -> Result<(), DomainError> {
    let mut old_end = 0usize;
    let mut new_end = 0usize;
    for change in changes {
        if change.old.start < old_end
            || change.new.start < new_end
            || change.old.start > change.old.end
            || change.new.start > change.new.end
            || change.old.end > before.len()
            || change.new.end > after.len()
            || !before.is_char_boundary(change.old.start)
            || !before.is_char_boundary(change.old.end)
            || !after.is_char_boundary(change.new.start)
            || !after.is_char_boundary(change.new.end)
            || before.get(old_end..change.old.start) != after.get(new_end..change.new.start)
        {
            return Err(DomainError::InvalidContentRange);
        }
        old_end = change.old.end;
        new_end = change.new.end;
    }
    if before.get(old_end..) != after.get(new_end..) {
        return Err(DomainError::InvalidContentRange);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Affinity {
    Before,
    After,
}

fn map_offset(
    offset: usize,
    changes: &[AnnotationTextChange],
    affinity: Affinity,
) -> Result<usize, DomainError> {
    let mut old_end = 0usize;
    let mut new_end = 0usize;
    for change in changes {
        if offset < change.old.start {
            return new_end
                .checked_add(offset - old_end)
                .ok_or(DomainError::ContentLengthOverflow);
        }
        if offset <= change.old.end {
            return Ok(match affinity {
                Affinity::Before => change.new.start,
                Affinity::After => change.new.end,
            });
        }
        old_end = change.old.end;
        new_end = change.new.end;
    }
    new_end
        .checked_add(offset - old_end)
        .ok_or(DomainError::ContentLengthOverflow)
}

fn intersects(annotation: &ContentAnnotation, old: &Range<usize>) -> bool {
    if old.is_empty() {
        annotation.start < old.start && old.start < annotation.end
    } else {
        old.start < annotation.end && annotation.start < old.end
    }
}

#[cfg(test)]
mod tests;
