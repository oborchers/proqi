use crate::domain::ThoughtId;

use super::{AttachmentAccessibilityState, AttachmentHealth};

/// User-visible transient state derived from one exact attachment health entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentPresentationState {
    /// Normal presentation, including neutral unknown and checking health.
    Available,
    /// Public macOS metadata proves the item remains in iCloud without a local copy.
    InCloud,
    /// Public macOS metadata proves the item is currently downloading.
    Downloading,
    /// Exact readability failed without authoritative iCloud availability metadata.
    Inaccessible,
}

impl AttachmentAccessibilityState {
    /// Return the presentation state for one current attachment annotation.
    #[must_use]
    pub fn presentation_state(
        &self,
        thought_id: ThoughtId,
        annotation_index: usize,
    ) -> AttachmentPresentationState {
        self.known
            .get(&thought_id)
            .and_then(|keys| {
                keys.iter()
                    .find(|key| key.annotation_index == annotation_index)
            })
            .and_then(|key| self.health.get(key))
            .map_or(
                AttachmentPresentationState::Available,
                |health| match health {
                    AttachmentHealth::Unknown
                    | AttachmentHealth::Checking
                    | AttachmentHealth::Available => AttachmentPresentationState::Available,
                    AttachmentHealth::InCloud => AttachmentPresentationState::InCloud,
                    AttachmentHealth::Downloading => AttachmentPresentationState::Downloading,
                    AttachmentHealth::Inaccessible(_) => AttachmentPresentationState::Inaccessible,
                },
            )
    }

    /// Whether one current annotation has generic inaccessible presentation.
    #[must_use]
    pub fn inaccessible(&self, thought_id: ThoughtId, annotation_index: usize) -> bool {
        self.presentation_state(thought_id, annotation_index)
            == AttachmentPresentationState::Inaccessible
    }
}
