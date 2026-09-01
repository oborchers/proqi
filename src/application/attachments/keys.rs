//! Exact attachment cache identities and refresh-state transitions.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest as _, Sha256};

use crate::{
    domain::{ContentAnnotationKind, SessionBoard, Thought, ThoughtId},
    ports::attachment_accessibility::AttachmentCheckKey,
};

use super::{AttachmentAccessibilityState, AttachmentHealth};

impl From<Result<(), crate::ports::attachment_accessibility::AttachmentAccessFailure>>
    for AttachmentHealth
{
    fn from(
        result: Result<(), crate::ports::attachment_accessibility::AttachmentAccessFailure>,
    ) -> Self {
        match result {
            Ok(()) => Self::Accessible,
            Err(failure) => Self::Inaccessible(failure),
        }
    }
}

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
    /// Whether the latest explicit refresh still owns a current exact snapshot.
    #[must_use]
    pub const fn manual_refresh_active(&self) -> bool {
        self.manual_refresh.is_some()
    }

    pub(super) fn seed_unknown(&mut self) {
        for key in self.known.values().flatten() {
            self.health
                .entry(key.clone())
                .or_insert(AttachmentHealth::Unknown);
        }
    }

    pub(super) fn mark_checking(&mut self, keys: &[AttachmentCheckKey]) {
        for key in keys {
            if !matches!(
                self.health.get(key),
                Some(AttachmentHealth::Accessible | AttachmentHealth::Inaccessible(_))
            ) {
                self.health.insert(key.clone(), AttachmentHealth::Checking);
            }
        }
    }

    /// Seed current exact keys from transient proof established by an insertion adapter.
    pub fn mark_paths_accessible(&mut self, thought_id: ThoughtId, paths: &[String]) {
        let Some(keys) = self.known.get(&thought_id) else {
            return;
        };
        for key in keys {
            if paths.contains(&key.canonical_path) {
                self.health
                    .insert(key.clone(), AttachmentHealth::Accessible);
            }
        }
    }

    /// Seed every current attachment in one adapter-verified screenshot thought.
    pub fn mark_thought_accessible(&mut self, thought_id: ThoughtId) {
        let Some(keys) = self.known.get(&thought_id) else {
            return;
        };
        for key in keys {
            self.health
                .insert(key.clone(), AttachmentHealth::Accessible);
        }
    }

    pub(super) fn has_result(&self, key: &AttachmentCheckKey) -> bool {
        matches!(
            self.health.get(key),
            Some(AttachmentHealth::Accessible | AttachmentHealth::Inaccessible(_))
        )
    }

    pub(super) fn retain_current_health(&mut self) {
        let current = self
            .known
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.health.retain(|key, _| current.contains(key));
    }

    pub(super) fn migrate_health(
        &mut self,
        current: &BTreeMap<ThoughtId, Vec<AttachmentCheckKey>>,
        changed: &BTreeSet<ThoughtId>,
    ) -> (BTreeMap<AttachmentCheckKey, AttachmentCheckKey>, bool) {
        let previous = self.health.clone();
        let mut replacements = BTreeMap::new();
        let mut identities_stable = true;
        self.health.retain(|key, _| {
            !changed.contains(&key.thought_id)
                && current
                    .get(&key.thought_id)
                    .is_some_and(|keys| keys.contains(key))
        });
        for thought_id in changed {
            let old_keys = self.known.get(thought_id).cloned().unwrap_or_default();
            let new_keys = current.get(thought_id).cloned().unwrap_or_default();
            let pairs = matching_key_pairs(&old_keys, &new_keys);
            identities_stable &= pairs.len() == old_keys.len() && pairs.len() == new_keys.len();
            self.health
                .extend(pairs.iter().filter_map(|(old_key, new_key)| {
                    previous
                        .get(old_key)
                        .copied()
                        .map(|health| (new_key.clone(), health))
                }));
            replacements.extend(pairs);
        }
        (replacements, identities_stable)
    }

    pub(super) fn remap_scheduled_keys(
        &mut self,
        replacements: &BTreeMap<AttachmentCheckKey, AttachmentCheckKey>,
    ) {
        let current = self
            .known
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.background = self
            .background
            .drain(..)
            .filter_map(|key| {
                replacements
                    .get(&key)
                    .cloned()
                    .or_else(|| current.contains(&key).then_some(key))
            })
            .collect();
        self.queued = self.background.iter().cloned().collect();
        if let Some(refresh) = &mut self.manual_refresh {
            refresh.pending = refresh
                .pending
                .iter()
                .filter_map(|key| {
                    replacements
                        .get(key)
                        .cloned()
                        .or_else(|| current.contains(key).then(|| key.clone()))
                })
                .collect();
        }
        for replacement in self.in_flight_replacements.values_mut() {
            if let Some(next) = replacements.get(replacement) {
                *replacement = next.clone();
            }
        }
        let aliases = self
            .in_flight
            .iter()
            .flat_map(|batch| batch.keys.iter())
            .filter_map(|key| {
                replacements
                    .get(key)
                    .map(|replacement| (key.clone(), replacement.clone()))
            });
        self.in_flight_replacements.extend(aliases);
    }

    pub(super) fn current_result_key(
        &self,
        key: &AttachmentCheckKey,
    ) -> Option<AttachmentCheckKey> {
        let candidate = self.in_flight_replacements.get(key).unwrap_or(key);
        self.known
            .get(&candidate.thought_id)
            .is_some_and(|keys| keys.contains(candidate))
            .then(|| candidate.clone())
    }

    pub(super) fn key_in_current_in_flight(&self, key: &AttachmentCheckKey) -> bool {
        self.in_flight.as_ref().is_some_and(|batch| {
            let current_generation = batch.purpose != super::AttachmentCheckPurpose::Background
                || batch.background_epoch == self.background_epoch;
            current_generation
                && batch.keys.iter().any(|candidate| {
                    candidate == key || self.in_flight_replacements.get(candidate) == Some(key)
                })
        })
    }
}

fn matching_key_pairs(
    old_keys: &[AttachmentCheckKey],
    new_keys: &[AttachmentCheckKey],
) -> Vec<(AttachmentCheckKey, AttachmentCheckKey)> {
    let mut used = BTreeSet::new();
    let mut migrated = vec![None; new_keys.len()];
    for (new_index, new_key) in new_keys.iter().enumerate() {
        if let Some(old_index) = old_keys
            .iter()
            .enumerate()
            .find_map(|(index, old_key)| (old_key == new_key).then_some(index))
        {
            used.insert(old_index);
            migrated[new_index] = Some(old_index);
        }
    }
    for (new_index, new_key) in new_keys.iter().enumerate() {
        if migrated[new_index].is_none() {
            migrated[new_index] = matching_index(old_keys, new_key, &mut used);
        }
    }
    new_keys
        .iter()
        .zip(migrated)
        .filter_map(|(new_key, old_index)| {
            old_index.map(|old_index| (old_keys[old_index].clone(), new_key.clone()))
        })
        .collect()
}

fn matching_index(
    old_keys: &[AttachmentCheckKey],
    new_key: &AttachmentCheckKey,
    used: &mut BTreeSet<usize>,
) -> Option<usize> {
    let index = old_keys.iter().enumerate().find_map(|(index, old_key)| {
        (!used.contains(&index) && same_attachment(old_key, new_key)).then_some(index)
    })?;
    used.insert(index);
    Some(index)
}

fn same_attachment(left: &AttachmentCheckKey, right: &AttachmentCheckKey) -> bool {
    left.thought_id == right.thought_id
        && left.image == right.image
        && left.display_name == right.display_name
        && left.canonical_path == right.canonical_path
}
