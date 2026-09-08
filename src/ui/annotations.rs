//! Exact paste payloads and folded board presentation.

use crate::application::AttachmentPresentationState;
use crate::domain::{
    AnnotationBehavior, ContentAnnotation, ContentAnnotationKind, InlineStyleKind,
};

mod rebase;
mod reflow;

pub(super) use rebase::{rebase, rebase_preserved};
pub(in crate::ui) use reflow::{PasteReflow, ReflowProjection};

const LARGE_PASTE_LINES: usize = 12;
const LARGE_PASTE_GRAPHEMES: usize = 1_200;

/// Exact inserted text with optional durable presentation provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PastePayload {
    /// Canonical text supplied to editing, copy, export, and submission.
    pub content: String,
    /// Presentation metadata over UTF-8 byte ranges in `content`.
    pub annotations: Vec<ContentAnnotation>,
    verified_paths: Vec<String>,
    preserve_owned_annotations: bool,
}

impl PastePayload {
    /// Preserve plain text and fold it only when it exceeds the context threshold.
    #[must_use]
    pub fn text(content: String) -> Self {
        let annotations = reflow::large_paste_annotation(&content, 0, content.len())
            .into_iter()
            .collect();
        Self {
            content,
            annotations,
            verified_paths: Vec::new(),
            preserve_owned_annotations: false,
        }
    }

    /// Construct a public annotated payload without permitting semantic style creation.
    ///
    /// # Errors
    ///
    /// Returns an annotation error when ranges are invalid or include application-owned
    /// shortcut emphasis.
    pub fn annotated(
        content: String,
        annotations: Vec<ContentAnnotation>,
    ) -> Result<Self, crate::domain::DomainError> {
        crate::domain::validate_annotations(&content, &annotations)?;
        if annotations
            .iter()
            .any(ContentAnnotation::is_shortcut_emphasis)
        {
            return Err(crate::domain::DomainError::InvalidContentAnnotation);
        }
        Ok(Self {
            content,
            annotations,
            verified_paths: Vec::new(),
            preserve_owned_annotations: false,
        })
    }

    /// Construct attachment substitutions derived by the terminal adapter.
    pub(crate) fn attachments(
        content: String,
        ranges: Vec<(std::ops::Range<usize>, bool, String)>,
    ) -> Self {
        let annotations = ranges
            .into_iter()
            .map(|(range, image, display_name)| ContentAnnotation {
                start: range.start,
                end: range.end,
                kind: ContentAnnotationKind::Attachment {
                    ordinal: None,
                    image,
                    display_name,
                },
            })
            .collect();
        Self {
            content,
            annotations,
            verified_paths: Vec::new(),
            preserve_owned_annotations: false,
        }
    }

    /// Construct one inert invocation-reference substitution owned by discovery.
    pub(crate) fn invocation_reference(
        content: String,
        range: std::ops::Range<usize>,
        display_name: String,
    ) -> Self {
        Self {
            content,
            annotations: vec![ContentAnnotation {
                start: range.start,
                end: range.end,
                kind: ContentAnnotationKind::InvocationReference { display_name },
            }],
            verified_paths: Vec::new(),
            preserve_owned_annotations: false,
        }
    }

    /// Construct metadata that a verified Proqi clipboard flavor preserved.
    pub(crate) fn preserved_clipboard(
        content: String,
        annotations: Vec<ContentAnnotation>,
    ) -> Result<Self, crate::domain::DomainError> {
        crate::domain::validate_annotations(&content, &annotations)?;
        Ok(Self {
            content,
            annotations,
            verified_paths: Vec::new(),
            preserve_owned_annotations: true,
        })
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

    pub(in crate::ui) fn into_parts(self) -> (String, Vec<ContentAnnotation>, Vec<String>, bool) {
        (
            self.content,
            self.annotations,
            self.verified_paths,
            self.preserve_owned_annotations,
        )
    }
}

/// One substituted range in projected presentation text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PresentedSubstitution {
    pub(super) annotation_index: usize,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) content_end: usize,
    pub(super) canonical_start: usize,
    pub(super) canonical_end: usize,
    pub(super) collapsed: bool,
    pub(super) health: AttachmentPresentationState,
}

/// Closed semantic style applied to a projected visible byte range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PresentedStyleKind {
    Annotation,
    ShortcutEmphasis,
    Warning,
}

/// One semantic style range over projected visible text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PresentedStyle {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) kind: PresentedStyleKind,
}

/// A projection failure means canonical annotation invariants were bypassed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectionError {
    InvalidAnnotationRange,
}

/// Display-only projection that never replaces canonical content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Presentation {
    pub(super) content: String,
    pub(super) substitutions: Vec<PresentedSubstitution>,
    pub(super) styles: Vec<PresentedStyle>,
}

impl Presentation {
    pub(super) const fn canonical(content: String) -> Self {
        Self {
            content,
            substitutions: Vec::new(),
            styles: Vec::new(),
        }
    }
}

pub(super) fn project_with_health(
    content: &str,
    annotations: &[ContentAnnotation],
    expanded: &[usize],
    mut health: impl FnMut(usize) -> AttachmentPresentationState,
) -> Result<Presentation, ProjectionError> {
    let mut projection = ProjectionBuilder::default();
    for (annotation_index, annotation) in annotations.iter().enumerate() {
        let attachment_health =
            if matches!(annotation.kind, ContentAnnotationKind::Attachment { .. }) {
                health(annotation_index)
            } else {
                AttachmentPresentationState::Available
            };
        projection.push_annotation(
            content,
            annotation_index,
            annotation,
            expanded.contains(&annotation_index),
            attachment_health,
        )?;
    }
    projection.finish(content)
}

#[derive(Default)]
struct ProjectionBuilder {
    output: String,
    substitutions: Vec<PresentedSubstitution>,
    styles: Vec<PresentedStyle>,
    cursor: usize,
}

impl ProjectionBuilder {
    fn push_annotation(
        &mut self,
        content: &str,
        annotation_index: usize,
        annotation: &ContentAnnotation,
        expanded: bool,
        health: AttachmentPresentationState,
    ) -> Result<(), ProjectionError> {
        let prefix = content
            .get(self.cursor..annotation.start)
            .ok_or(ProjectionError::InvalidAnnotationRange)?;
        self.output.push_str(prefix);
        let start = self.output.len();
        match annotation.kind.behavior() {
            AnnotationBehavior::InlineStyle(kind) => {
                self.push_inline(content, annotation, start, kind)?;
            }
            AnnotationBehavior::Substitution if expanded => {
                self.push_expanded(content, annotation_index, annotation, start, health)?;
            }
            AnnotationBehavior::Substitution => {
                self.push_collapsed(annotation_index, annotation, start, health)?;
            }
        }
        self.cursor = annotation.end;
        Ok(())
    }

    fn push_inline(
        &mut self,
        content: &str,
        annotation: &ContentAnnotation,
        start: usize,
        kind: InlineStyleKind,
    ) -> Result<(), ProjectionError> {
        let exact = content
            .get(annotation.start..annotation.end)
            .ok_or(ProjectionError::InvalidAnnotationRange)?;
        self.output.push_str(exact);
        self.styles.push(PresentedStyle {
            start,
            end: self.output.len(),
            kind: match kind {
                InlineStyleKind::ShortcutEmphasis => PresentedStyleKind::ShortcutEmphasis,
            },
        });
        Ok(())
    }

    fn push_expanded(
        &mut self,
        content: &str,
        annotation_index: usize,
        annotation: &ContentAnnotation,
        start: usize,
        health: AttachmentPresentationState,
    ) -> Result<(), ProjectionError> {
        let exact = content
            .get(annotation.start..annotation.end)
            .ok_or(ProjectionError::InvalidAnnotationRange)?;
        self.output.push_str(exact);
        let content_end = self.output.len();
        push_expanded_health_suffix(&mut self.output, health);
        self.substitutions.push(substitution(
            annotation_index,
            start,
            self.output.len(),
            content_end,
            annotation,
            false,
            health,
        ));
        if health != AttachmentPresentationState::Available {
            self.styles.push(PresentedStyle {
                start,
                end: self.output.len(),
                kind: PresentedStyleKind::Warning,
            });
        }
        Ok(())
    }

    fn push_collapsed(
        &mut self,
        annotation_index: usize,
        annotation: &ContentAnnotation,
        start: usize,
        health: AttachmentPresentationState,
    ) -> Result<(), ProjectionError> {
        push_collapsed_label(&mut self.output, &annotation.kind, health)?;
        self.substitutions.push(substitution(
            annotation_index,
            start,
            self.output.len(),
            self.output.len(),
            annotation,
            true,
            health,
        ));
        self.styles.push(PresentedStyle {
            start,
            end: self.output.len(),
            kind: if health == AttachmentPresentationState::Available {
                PresentedStyleKind::Annotation
            } else {
                PresentedStyleKind::Warning
            },
        });
        Ok(())
    }

    fn finish(mut self, content: &str) -> Result<Presentation, ProjectionError> {
        let suffix = content
            .get(self.cursor..)
            .ok_or(ProjectionError::InvalidAnnotationRange)?;
        self.output.push_str(suffix);
        Ok(Presentation {
            content: self.output,
            substitutions: self.substitutions,
            styles: self.styles,
        })
    }
}

fn push_collapsed_label(
    output: &mut String,
    kind: &ContentAnnotationKind,
    health: AttachmentPresentationState,
) -> Result<(), ProjectionError> {
    match kind {
        ContentAnnotationKind::Attachment { image, ordinal, .. } => {
            let label = if *image { "Image" } else { "File" };
            let number = ordinal
                .ok_or(ProjectionError::InvalidAnnotationRange)?
                .get();
            output.push('[');
            output.push_str(label);
            output.push(' ');
            output.push_str(&number.to_string());
            push_collapsed_health_suffix(output, health);
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
        ContentAnnotationKind::ShortcutEmphasis(_) => {}
    }
    Ok(())
}

fn substitution(
    annotation_index: usize,
    start: usize,
    end: usize,
    content_end: usize,
    annotation: &ContentAnnotation,
    collapsed: bool,
    health: AttachmentPresentationState,
) -> PresentedSubstitution {
    PresentedSubstitution {
        annotation_index,
        start,
        end,
        content_end,
        canonical_start: annotation.start,
        canonical_end: annotation.end,
        collapsed,
        health,
    }
}

fn push_collapsed_health_suffix(output: &mut String, health: AttachmentPresentationState) {
    match health {
        AttachmentPresentationState::Available => {}
        AttachmentPresentationState::InCloud => output.push_str(" · in iCloud"),
        AttachmentPresentationState::Downloading => output.push_str(" · downloading"),
        AttachmentPresentationState::Inaccessible => output.push_str(" · inaccessible"),
    }
}

fn push_expanded_health_suffix(output: &mut String, health: AttachmentPresentationState) {
    match health {
        AttachmentPresentationState::Available => {}
        AttachmentPresentationState::InCloud => output.push_str(" [in iCloud]"),
        AttachmentPresentationState::Downloading => output.push_str(" [downloading]"),
        AttachmentPresentationState::Inaccessible => output.push_str(" [inaccessible]"),
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
