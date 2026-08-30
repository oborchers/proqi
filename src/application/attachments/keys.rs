//! Exact attachment cache identities and refresh-state transitions.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest as _, Sha256};

use crate::{
    domain::{ContentAnnotationKind, SessionBoard, Thought, ThoughtId},
    ports::attachment_accessibility::AttachmentCheckKey,
};

use super::{AttachmentAccessibilityState, AttachmentHealth};

/// Extract exact transient cache keys from one source thought.
#[must_use]
pub fn attachment_keys(thought: &Thought) -> Vec<AttachmentCheckKey> {
    let revision: [u8; 32] = Sha256::digest(thought.content.as_bytes()).into();
    thought
        .annotations
        .iter()
        .enumerate()
        .filter_map(|(annotation_index, annotation)| {
            let ContentAnnotationKind::Attachment {
                image,
                display_name,
            } = &annotation.kind
            else {
                return None;
            };
            let canonical_path = thought.content.get(annotation.start..annotation.end)?;
            Some(AttachmentCheckKey {
                thought_id: thought.id,
                annotation_index,
                annotation_start: annotation.start,
                annotation_end: annotation.end,
                image: *image,
                display_name: display_name.clone(),
                canonical_path: canonical_path.to_owned(),
                content_revision: revision,
            })
        })
        .collect()
}

pub(super) fn attachment_keys_by_thought(
    board: &SessionBoard,
) -> BTreeMap<ThoughtId, Vec<AttachmentCheckKey>> {
    board
        .live_thoughts()
        .into_iter()
        .map(|thought| (thought.id, attachment_keys(thought)))
        .collect()
}

impl AttachmentAccessibilityState {
    pub(super) fn has_result(&self, key: &AttachmentCheckKey) -> bool {
        matches!(
            self.health.get(key),
            Some(AttachmentHealth::Accessible | AttachmentHealth::Inaccessible(_))
        )
    }

    pub(super) fn seed_unverified(&mut self) {
        for key in self.known.values().flatten() {
            self.health
                .entry(key.clone())
                .or_insert(AttachmentHealth::Unverified);
        }
    }

    pub(super) fn demote_for_refresh(&mut self) {
        let current = self
            .known
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.health.retain(|key, _| current.contains(key));
        for key in current {
            let next = match self.health.get(&key) {
                Some(
                    AttachmentHealth::Inaccessible(failure)
                    | AttachmentHealth::Checking(Some(failure)),
                ) => AttachmentHealth::Inaccessible(*failure),
                _ => AttachmentHealth::Unverified,
            };
            self.health.insert(key, next);
        }
    }
}
