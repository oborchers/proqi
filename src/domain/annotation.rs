//! Durable presentation metadata over exact canonical thought content.

use serde::{Deserialize, Serialize};

use super::model::DomainError;

const MAX_INVOCATION_REFERENCE_LABEL_CHARS: usize = 256;

/// Closed rendering behavior owned by one durable annotation kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationBehavior {
    /// Replace the canonical range with a presentation-only label.
    Substitution,
    /// Preserve every character and apply one semantic inline role.
    InlineStyle(InlineStyleKind),
}

/// Closed semantic inline roles understood by Proqi.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineStyleKind {
    /// Application-authored instructional shortcut text.
    ShortcutEmphasis,
}

/// Construction marker for application-authored shortcut emphasis.
///
/// Its empty durable representation stores no color, style name, display value,
/// or provenance claim. Supported mutation surfaces enforce creation authority;
/// direct same-user database tampering is outside that guarantee.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutEmphasis {
    #[serde(skip)]
    private: (),
}

impl ShortcutEmphasis {
    pub(crate) const fn application_owned() -> Self {
        Self { private: () }
    }
}

/// Durable presentation metadata for one exact content range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentAnnotation {
    /// Inclusive UTF-8 byte offset in the canonical thought content.
    pub start: usize,
    /// Exclusive UTF-8 byte offset in the canonical thought content.
    pub end: usize,
    /// Presentation origin retained independently from the text.
    pub kind: ContentAnnotationKind,
}

/// Provenance used to fold context without rewriting canonical content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum ContentAnnotationKind {
    /// One absolute local file path.
    Attachment {
        /// Whether the file should receive image-specific presentation.
        image: bool,
        /// Safe basename shown instead of the complete path.
        display_name: String,
    },
    /// One large terminal or clipboard paste.
    LargePaste {
        /// Logical line count at capture time.
        lines: usize,
        /// Perceived Unicode character count at capture time.
        graphemes: usize,
    },
    /// One compact collaborator mention over an exact plain-text location.
    InvocationReference {
        /// Bounded display label beginning with the visible `@` cue.
        display_name: String,
    },
    /// Exact application-authored instructional shortcut text.
    ShortcutEmphasis(ShortcutEmphasis),
}

impl ContentAnnotationKind {
    /// Return the exhaustive projection behavior for this durable kind.
    #[must_use]
    pub const fn behavior(&self) -> AnnotationBehavior {
        match self {
            Self::Attachment { .. }
            | Self::LargePaste { .. }
            | Self::InvocationReference { .. } => AnnotationBehavior::Substitution,
            Self::ShortcutEmphasis(_) => {
                AnnotationBehavior::InlineStyle(InlineStyleKind::ShortcutEmphasis)
            }
        }
    }
}

impl ContentAnnotation {
    pub(crate) fn shortcut(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            kind: ContentAnnotationKind::ShortcutEmphasis(ShortcutEmphasis::application_owned()),
        }
    }

    /// Whether this range is application-authored semantic shortcut emphasis.
    #[must_use]
    pub const fn is_shortcut_emphasis(&self) -> bool {
        matches!(self.kind, ContentAnnotationKind::ShortcutEmphasis(_))
    }
}

/// Validate sorted non-overlapping annotation ranges against canonical content.
///
/// # Errors
///
/// Returns [`DomainError::InvalidContentAnnotation`] when a range is invalid or
/// an invocation-reference label cannot be rendered safely.
pub fn validate_annotations(
    content: &str,
    annotations: &[ContentAnnotation],
) -> Result<(), DomainError> {
    let mut previous_end = 0;
    for annotation in annotations {
        if invalid_range(content, annotation, previous_end)
            || invalid_invocation_label(&annotation.kind)
        {
            return Err(DomainError::InvalidContentAnnotation);
        }
        previous_end = annotation.end;
    }
    Ok(())
}

fn invalid_range(content: &str, annotation: &ContentAnnotation, previous_end: usize) -> bool {
    annotation.start >= annotation.end
        || annotation.end > content.len()
        || !content.is_char_boundary(annotation.start)
        || !content.is_char_boundary(annotation.end)
        || annotation.start < previous_end
}

fn invalid_invocation_label(kind: &ContentAnnotationKind) -> bool {
    let ContentAnnotationKind::InvocationReference { display_name } = kind else {
        return false;
    };
    display_name.trim() != display_name
        || !display_name.starts_with('@')
        || display_name.chars().count() <= 1
        || display_name.chars().count() > MAX_INVOCATION_REFERENCE_LABEL_CHARS
        || display_name.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invocation(display_name: &str) -> ContentAnnotation {
        ContentAnnotation {
            start: 0,
            end: 1,
            kind: ContentAnnotationKind::InvocationReference {
                display_name: display_name.to_owned(),
            },
        }
    }

    #[test]
    fn bounded_unicode_invocation_labels_are_valid() {
        assert_eq!(
            validate_annotations("x", &[invocation("@協作者 · codex")]),
            Ok(())
        );
    }

    #[test]
    fn attachment_range_can_preserve_unannotated_trailing_content() {
        let path = "/tmp/Grüße 🖼️.png";
        let content = format!("{path} ");
        let annotation = ContentAnnotation {
            start: 0,
            end: path.len(),
            kind: ContentAnnotationKind::Attachment {
                image: true,
                display_name: "Grüße 🖼️.png".to_owned(),
            },
        };

        assert_eq!(validate_annotations(&content, &[annotation]), Ok(()));
    }

    #[test]
    fn unsafe_or_unhelpful_invocation_labels_fail_closed() {
        for label in ["@", " reviewer", "@reviewer\nsecret"] {
            assert_eq!(
                validate_annotations("x", &[invocation(label)]),
                Err(DomainError::InvalidContentAnnotation)
            );
        }
        let oversized = format!("@{}", "a".repeat(MAX_INVOCATION_REFERENCE_LABEL_CHARS));
        assert_eq!(
            validate_annotations("x", &[invocation(&oversized)]),
            Err(DomainError::InvalidContentAnnotation)
        );
    }

    #[test]
    fn shortcut_serialization_contains_only_closed_semantics_and_range() {
        let encoded = serde_json::to_value(ContentAnnotation::shortcut(2, 5))
            .expect("serialize shortcut emphasis");
        assert_eq!(
            encoded,
            serde_json::json!({
                "start": 2,
                "end": 5,
                "kind": { "kind": "shortcut_emphasis" }
            })
        );
    }

    #[test]
    fn unknown_or_payload_bearing_shortcut_kinds_fail_closed() {
        for encoded in [
            r#"{"start":0,"end":1,"kind":{"kind":"future_style"}}"#,
            r#"{"start":0,"end":1,"kind":{"kind":"shortcut_emphasis","color":"red"}}"#,
        ] {
            assert!(serde_json::from_str::<ContentAnnotation>(encoded).is_err());
        }
    }
}
