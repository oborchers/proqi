//! Session-scoped attachment occurrence numbering, independent of text and filesystem state.

use serde::{Deserialize, Serialize};

use super::{ContentAnnotation, ContentAnnotationKind, DomainError};

/// Positive durable ordinal within one session and attachment kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct AttachmentOrdinal(u64);

impl AttachmentOrdinal {
    /// Read the assigned positive number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for AttachmentOrdinal {
    type Error = DomainError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 || value > i64::MAX as u64 {
            return Err(DomainError::InvalidContentAnnotation);
        }
        Ok(Self(value))
    }
}

impl From<AttachmentOrdinal> for u64 {
    fn from(value: AttachmentOrdinal) -> Self {
        value.0
    }
}

/// Monotonic high-water marks, retained independently of current content and undo cursors.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttachmentCounters {
    image: u64,
    file: u64,
}

impl AttachmentCounters {
    /// Observe every assigned snapshot in a retained mutation, including inverse-only state.
    ///
    /// # Errors
    /// Rejects unassigned durable occurrences.
    pub fn observe_mutation(&mut self, mutation: &super::BoardMutation) -> Result<(), DomainError> {
        use super::BoardMutation;
        match mutation {
            BoardMutation::Batch { mutations } => {
                for mutation in mutations {
                    self.observe_mutation(mutation)?;
                }
            }
            BoardMutation::AddThought { thought } => self.observe(&thought.annotations)?,
            BoardMutation::ReplaceContent {
                before_annotations,
                after_annotations,
                ..
            } => {
                self.observe(before_annotations)?;
                self.observe(after_annotations)?;
            }
            BoardMutation::SetDeletionExact {
                expected_annotations,
                ..
            } => self.observe(expected_annotations)?,
            BoardMutation::SetDeletion { .. }
            | BoardMutation::MoveThought { .. }
            | BoardMutation::SetPresentation { .. }
            | BoardMutation::LegacySetCollapsed { .. } => {}
        }
        Ok(())
    }
    /// Restore validated durable high-water marks. Zero means no allocation yet.
    ///
    /// # Errors
    /// Rejects values beyond the durable integer range.
    pub fn new(image: u64, file: u64) -> Result<Self, DomainError> {
        if image > i64::MAX as u64 || file > i64::MAX as u64 {
            return Err(DomainError::InvalidContentAnnotation);
        }
        Ok(Self { image, file })
    }

    /// Last allocated image ordinal, including deleted and dormant occurrences.
    #[must_use]
    pub const fn image(self) -> u64 {
        self.image
    }

    /// Last allocated file ordinal, including deleted and dormant occurrences.
    #[must_use]
    pub const fn file(self) -> u64 {
        self.file
    }

    fn counter_mut(&mut self, image: bool) -> &mut u64 {
        if image {
            &mut self.image
        } else {
            &mut self.file
        }
    }

    /// Allocate each unassigned occurrence atomically in annotation order.
    ///
    /// # Errors
    /// Returns an annotation error on sequence exhaustion without changing either argument.
    pub fn assign(&mut self, annotations: &mut [ContentAnnotation]) -> Result<(), DomainError> {
        let mut candidate = *self;
        let mut assigned = annotations.to_vec();
        for annotation in &mut assigned {
            if let ContentAnnotationKind::Attachment { image, ordinal, .. } = &mut annotation.kind
                && ordinal.is_none()
            {
                let counter = candidate.counter_mut(*image);
                let next = counter
                    .checked_add(1)
                    .ok_or(DomainError::InvalidContentAnnotation)?;
                *ordinal = Some(AttachmentOrdinal::try_from(next)?);
                *counter = next;
            }
        }
        annotations.clone_from_slice(&assigned);
        *self = candidate;
        Ok(())
    }

    /// Observe an already assigned snapshot without allocating or decreasing a counter.
    ///
    /// # Errors
    /// Rejects an unassigned attachment in durable state.
    pub fn observe(&mut self, annotations: &[ContentAnnotation]) -> Result<(), DomainError> {
        for annotation in annotations {
            if let ContentAnnotationKind::Attachment { image, ordinal, .. } = &annotation.kind {
                let ordinal = ordinal.ok_or(DomainError::InvalidContentAnnotation)?.get();
                let counter = self.counter_mut(*image);
                *counter = (*counter).max(ordinal);
            }
        }
        Ok(())
    }
}

pub(super) fn validate_unique(
    annotations: &[ContentAnnotation],
    identities: &mut std::collections::HashSet<(bool, AttachmentOrdinal)>,
) -> Result<(), DomainError> {
    for annotation in annotations {
        let ContentAnnotationKind::Attachment { image, ordinal, .. } = &annotation.kind else {
            continue;
        };
        let ordinal = ordinal.ok_or(DomainError::InvalidContentAnnotation)?;
        if !identities.insert((*image, ordinal)) {
            return Err(DomainError::InvalidContentAnnotation);
        }
    }
    Ok(())
}

/// Mark copied input as new occurrences before destination allocation.
pub fn renew_attachment_occurrences(annotations: &mut [ContentAnnotation]) {
    for annotation in annotations {
        if let ContentAnnotationKind::Attachment { ordinal, .. } = &mut annotation.kind {
            *ordinal = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(image: bool) -> ContentAnnotation {
        ContentAnnotation {
            start: 0,
            end: 1,
            kind: ContentAnnotationKind::Attachment {
                ordinal: None,
                image,
                display_name: "asset".to_owned(),
            },
        }
    }

    #[test]
    fn sequences_are_independent_and_retained_identity_never_reallocates() {
        let mut counters = AttachmentCounters::default();
        let mut annotations = vec![attachment(true), attachment(false), attachment(true)];
        counters.assign(&mut annotations).expect("assign");
        assert_eq!((counters.image(), counters.file()), (2, 1));
        let saved = annotations.clone();
        counters.assign(&mut annotations).expect("preserve");
        assert_eq!(annotations, saved);
        renew_attachment_occurrences(&mut annotations);
        counters.assign(&mut annotations).expect("new occurrences");
        assert_eq!((counters.image(), counters.file()), (4, 2));
        assert_ne!(annotations, saved);
    }

    #[test]
    fn overflow_rolls_back_the_complete_mixed_allocation() {
        let mut counters = AttachmentCounters::new(i64::MAX as u64, 3).expect("counters");
        let mut annotations = vec![attachment(false), attachment(true)];
        let before = annotations.clone();
        assert_eq!(
            counters.assign(&mut annotations),
            Err(DomainError::InvalidContentAnnotation)
        );
        assert_eq!(annotations, before);
        assert_eq!(counters.file(), 3);
    }

    #[test]
    fn durable_ordinal_decoding_rejects_zero_negative_fraction_and_overflow() {
        for encoded in ["0", "-1", "1.5", "9223372036854775808", "null"] {
            assert!(
                serde_json::from_str::<AttachmentOrdinal>(encoded).is_err(),
                "{encoded}"
            );
        }
        assert_eq!(
            serde_json::from_str::<AttachmentOrdinal>("9223372036854775807")
                .expect("maximum")
                .get(),
            i64::MAX as u64
        );
    }
}
