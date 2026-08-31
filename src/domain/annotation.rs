//! Durable presentation metadata over exact canonical thought content.

use serde::{Deserialize, Serialize};

use super::model::DomainError;

const MAX_INVOCATION_REFERENCE_LABEL_CHARS: usize = 256;

/// Durable presentation metadata for one exact content range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case", tag = "kind")]
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
}
