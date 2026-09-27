//! Paste-payload projection over the application-owned spacing-cleanup policy.

use super::PastePayload;

#[cfg(test)]
pub(in crate::ui) use crate::application::text_reflow::ReflowedAnnotations;

#[cfg(test)]
mod tests;

pub(in crate::ui) enum PasteReflow {
    Changed(PastePayload),
    Unchanged,
    Empty,
}

pub(in crate::ui) struct ReflowProjection {
    pub(in crate::ui) outcome: PasteReflow,
    pub(in crate::ui) changes: crate::ports::editor::TextChangeSet,
    pub(in crate::ui) annotation_origins: Vec<(usize, usize)>,
}

impl PastePayload {
    pub(in crate::ui) fn reflow(&self) -> Result<PasteReflow, ()> {
        self.reflow_with_changes()
            .map(|projection| projection.outcome)
    }

    pub(in crate::ui) fn reflow_with_changes(&self) -> Result<ReflowProjection, ()> {
        let projection = crate::application::text_reflow::reflow(&self.content, &self.annotations)?;
        let outcome = match projection.outcome {
            crate::application::text_reflow::TextReflowOutcome::Changed {
                content,
                annotations,
            } => PasteReflow::Changed(Self {
                content,
                annotations,
                verified_paths: self.verified_paths.clone(),
                preserve_owned_annotations: self.preserve_owned_annotations,
            }),
            crate::application::text_reflow::TextReflowOutcome::Unchanged => PasteReflow::Unchanged,
            crate::application::text_reflow::TextReflowOutcome::Empty => PasteReflow::Empty,
        };
        Ok(ReflowProjection {
            outcome,
            changes: projection.changes,
            annotation_origins: projection.annotation_origins,
        })
    }
}

pub(super) fn large_paste_annotation(
    content: &str,
    start: usize,
    end: usize,
) -> Option<crate::domain::ContentAnnotation> {
    crate::application::text_reflow::large_paste_annotation(content, start, end)
}

#[cfg(test)]
fn reflow_annotations(
    payload: &PastePayload,
    transformed: &crate::application::text_reflow::format::ReflowedText,
) -> Result<ReflowedAnnotations, crate::domain::DomainError> {
    crate::application::text_reflow::reflow_annotations(
        &payload.content,
        &payload.annotations,
        transformed,
    )
}
