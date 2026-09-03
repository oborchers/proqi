//! Annotation rebasing policy for ordinary and verified owned insertions.

use crate::{
    domain::{AnnotationTextChange, ContentAnnotation, rebase_annotations},
    ports::editor::TextChangeSet,
};

pub(in crate::ui) fn rebase(
    before: &str,
    after: &str,
    changes: &TextChangeSet,
    annotations: &[ContentAnnotation],
    inserted: &[ContentAnnotation],
) -> Vec<ContentAnnotation> {
    rebase_with_policy(before, after, changes, annotations, inserted, false)
}

pub(in crate::ui) fn rebase_preserved(
    before: &str,
    after: &str,
    changes: &TextChangeSet,
    annotations: &[ContentAnnotation],
    inserted: &[ContentAnnotation],
) -> Vec<ContentAnnotation> {
    rebase_with_policy(before, after, changes, annotations, inserted, true)
}

fn rebase_with_policy(
    before: &str,
    after: &str,
    changes: &TextChangeSet,
    annotations: &[ContentAnnotation],
    inserted: &[ContentAnnotation],
    preserve_owned: bool,
) -> Vec<ContentAnnotation> {
    let changes = changes
        .as_slice()
        .iter()
        .map(|change| AnnotationTextChange {
            old: change.old_range(),
            new: change.new_range(),
        })
        .collect::<Vec<_>>();
    let inserted = inserted
        .iter()
        .filter(|annotation| preserve_owned || !annotation.is_shortcut_emphasis())
        .cloned()
        .collect::<Vec<_>>();
    annotations_or_empty(rebase_annotations(
        before,
        after,
        &changes,
        annotations,
        &inserted,
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
