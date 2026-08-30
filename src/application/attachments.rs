//! Transient attachment health, bounded scheduling, and submission preflight policy.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Duration,
};

use sha2::{Digest as _, Sha256};

use crate::{
    domain::{ContentAnnotationKind, SessionBoard, SubmissionId, Thought, ThoughtId},
    ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentCheckBatch, AttachmentCheckBatchResult,
        AttachmentCheckKey, AttachmentCheckPurpose, AttachmentCheckResult,
    },
};

use super::Effect;

const BACKGROUND_BATCH_SIZE: usize = 16;
const PREFLIGHT_BATCH_SIZE: usize = 32;
const BACKGROUND_TIMEOUT: Duration = Duration::from_secs(2);
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(5);
const INACTIVE_REFRESH_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentHealth {
    Accessible,
    Inaccessible(AttachmentAccessFailure),
}

impl From<Result<(), AttachmentAccessFailure>> for AttachmentHealth {
    fn from(result: Result<(), AttachmentAccessFailure>) -> Self {
        match result {
            Ok(()) => Self::Accessible,
            Err(failure) => Self::Inaccessible(failure),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InFlight {
    id: u64,
    purpose: AttachmentCheckPurpose,
    keys: Vec<AttachmentCheckKey>,
    background_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SubmissionPreflight {
    remaining: VecDeque<AttachmentCheckKey>,
    inaccessible: usize,
}

/// Completed mandatory preflight returned to the submission policy owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentPreflightOutcome {
    /// Submission whose exact captured sources were checked.
    pub submission_id: SubmissionId,
    /// Number of inaccessible or unverifiable annotations.
    pub inaccessible: usize,
}

/// Application-owned transient attachment state. Nothing here is persisted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentAccessibilityState {
    known: BTreeMap<ThoughtId, Vec<AttachmentCheckKey>>,
    health: BTreeMap<AttachmentCheckKey, AttachmentHealth>,
    background: VecDeque<AttachmentCheckKey>,
    queued: BTreeSet<AttachmentCheckKey>,
    preflights: BTreeMap<SubmissionId, SubmissionPreflight>,
    in_flight: Option<InFlight>,
    next_batch_id: u64,
    background_epoch: u64,
    last_deliberate: Option<Duration>,
    refreshed_since_last_deliberate: bool,
}

impl Default for AttachmentAccessibilityState {
    fn default() -> Self {
        Self {
            known: BTreeMap::new(),
            health: BTreeMap::new(),
            background: VecDeque::new(),
            queued: BTreeSet::new(),
            preflights: BTreeMap::new(),
            in_flight: None,
            next_batch_id: 1,
            background_epoch: 0,
            last_deliberate: None,
            refreshed_since_last_deliberate: false,
        }
    }
}

impl AttachmentAccessibilityState {
    /// Seed transient identities and schedule focused-first restoration checks.
    pub fn start(
        &mut self,
        board: &SessionBoard,
        focused: Option<ThoughtId>,
        now: Duration,
    ) -> Vec<Effect> {
        self.last_deliberate = Some(now);
        self.known = attachment_keys_by_thought(board);
        self.enqueue_board(board, focused, false);
        self.schedule()
    }

    /// Reconcile exact keys after canonical content or annotations mutate.
    pub fn reconcile(&mut self, board: &SessionBoard) -> Vec<Effect> {
        let current = attachment_keys_by_thought(board);
        let changed = self
            .known
            .keys()
            .chain(current.keys())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|id| self.known.get(id) != current.get(id))
            .collect::<BTreeSet<_>>();
        if changed.is_empty() {
            return Vec::new();
        }
        self.health
            .retain(|key, _| !changed.contains(&key.thought_id));
        self.background
            .retain(|key| !changed.contains(&key.thought_id));
        self.queued.retain(|key| !changed.contains(&key.thought_id));
        self.known = current;
        for thought_id in changed.into_iter().rev() {
            self.enqueue_thought_front(thought_id, false);
        }
        self.schedule()
    }

    /// Prioritize unknown health only after a real thought-focus transition.
    pub fn prioritize_focus(&mut self, thought_id: ThoughtId) -> Vec<Effect> {
        self.enqueue_thought_front(thought_id, false);
        self.schedule()
    }

    /// Invalidate and refresh every current attachment without polling.
    pub fn refresh_all(&mut self, board: &SessionBoard, focused: Option<ThoughtId>) -> Vec<Effect> {
        self.background_epoch = self.background_epoch.wrapping_add(1);
        self.health.clear();
        self.background.clear();
        self.queued.clear();
        self.known = attachment_keys_by_thought(board);
        self.refreshed_since_last_deliberate = true;
        self.enqueue_board(board, focused, true);
        self.schedule()
    }

    /// Apply the documented first-interaction fallback after bounded inactivity.
    pub fn note_deliberate_interaction(
        &mut self,
        board: &SessionBoard,
        focused: Option<ThoughtId>,
        now: Duration,
    ) -> Vec<Effect> {
        let should_refresh = self.last_deliberate.is_some_and(|previous| {
            now.saturating_sub(previous) >= INACTIVE_REFRESH_AFTER
                && !self.refreshed_since_last_deliberate
        });
        self.last_deliberate = Some(now);
        self.refreshed_since_last_deliberate = false;
        if should_refresh {
            self.refresh_all(board, focused)
        } else {
            Vec::new()
        }
    }

    /// Queue a fresh mandatory check of an exact captured attachment set.
    pub fn begin_preflight(
        &mut self,
        submission_id: SubmissionId,
        keys: Vec<AttachmentCheckKey>,
    ) -> (Vec<Effect>, Option<AttachmentPreflightOutcome>) {
        if keys.is_empty() {
            return (
                Vec::new(),
                Some(AttachmentPreflightOutcome {
                    submission_id,
                    inaccessible: 0,
                }),
            );
        }
        self.preflights.insert(
            submission_id,
            SubmissionPreflight {
                remaining: keys.into(),
                inaccessible: 0,
            },
        );
        (self.schedule(), None)
    }

    /// Apply one bounded completion, reject stale keys, and continue queued work.
    pub fn complete(
        &mut self,
        completion: AttachmentCheckBatchResult,
    ) -> (Vec<Effect>, Option<AttachmentPreflightOutcome>) {
        let AttachmentCheckBatchResult {
            id,
            purpose,
            results,
        } = completion;
        let Some(in_flight) = self.in_flight.take() else {
            return (Vec::new(), None);
        };
        if id != in_flight.id || purpose != in_flight.purpose {
            self.in_flight = Some(in_flight);
            return (Vec::new(), None);
        }
        let failures = self.apply_results(&in_flight, &results);
        let outcome = match in_flight.purpose {
            AttachmentCheckPurpose::Background => None,
            AttachmentCheckPurpose::SubmissionPreflight(submission_id) => {
                self.finish_preflight_batch(submission_id, failures)
            }
        };
        let effects = self.schedule();
        (effects, outcome)
    }

    /// Binary user-visible health for one current attachment annotation.
    #[must_use]
    pub fn inaccessible(&self, thought_id: ThoughtId, annotation_index: usize) -> bool {
        self.known
            .get(&thought_id)
            .and_then(|keys| {
                keys.iter()
                    .find(|key| key.annotation_index == annotation_index)
            })
            .and_then(|key| self.health.get(key))
            .is_some_and(|health| matches!(health, AttachmentHealth::Inaccessible(_)))
    }

    /// Exact current keys for source capture and fresh preflight.
    #[must_use]
    pub fn keys_for(&self, thought_ids: &[ThoughtId]) -> Option<Vec<AttachmentCheckKey>> {
        let mut keys = Vec::new();
        for thought_id in thought_ids {
            keys.extend(self.known.get(thought_id)?.iter().cloned());
        }
        Some(keys)
    }

    fn apply_results(&mut self, in_flight: &InFlight, results: &[AttachmentCheckResult]) -> usize {
        let returned = results
            .iter()
            .map(|result| (result.key.clone(), result.result))
            .collect::<BTreeMap<_, _>>();
        let mut failures = 0;
        for key in &in_flight.keys {
            let result = returned
                .get(key)
                .copied()
                .unwrap_or(Err(AttachmentAccessFailure::Io));
            if result.is_err() {
                failures += 1;
            }
            let current = self
                .known
                .get(&key.thought_id)
                .is_some_and(|keys| keys.contains(key));
            let fresh_background = in_flight.purpose != AttachmentCheckPurpose::Background
                || in_flight.background_epoch == self.background_epoch;
            if current && fresh_background {
                self.health.insert(key.clone(), result.into());
            }
        }
        failures
    }

    fn finish_preflight_batch(
        &mut self,
        submission_id: SubmissionId,
        failures: usize,
    ) -> Option<AttachmentPreflightOutcome> {
        let preflight = self.preflights.get_mut(&submission_id)?;
        preflight.inaccessible = preflight.inaccessible.saturating_add(failures);
        if !preflight.remaining.is_empty() {
            return None;
        }
        let inaccessible = preflight.inaccessible;
        self.preflights.remove(&submission_id);
        Some(AttachmentPreflightOutcome {
            submission_id,
            inaccessible,
        })
    }

    fn schedule(&mut self) -> Vec<Effect> {
        if self.in_flight.is_some() {
            return Vec::new();
        }
        let purpose = self.preflights.keys().next().copied().map_or(
            AttachmentCheckPurpose::Background,
            AttachmentCheckPurpose::SubmissionPreflight,
        );
        let (keys, timeout) = match purpose {
            AttachmentCheckPurpose::Background => {
                let keys = take_front(&mut self.background, BACKGROUND_BATCH_SIZE);
                for key in &keys {
                    self.queued.remove(key);
                }
                (keys, BACKGROUND_TIMEOUT)
            }
            AttachmentCheckPurpose::SubmissionPreflight(submission_id) => {
                let Some(preflight) = self.preflights.get_mut(&submission_id) else {
                    return Vec::new();
                };
                (
                    take_front(&mut preflight.remaining, PREFLIGHT_BATCH_SIZE),
                    PREFLIGHT_TIMEOUT,
                )
            }
        };
        if keys.is_empty() {
            return Vec::new();
        }
        let id = self.next_batch_id;
        self.next_batch_id = self.next_batch_id.wrapping_add(1).max(1);
        self.in_flight = Some(InFlight {
            id,
            purpose,
            keys: keys.clone(),
            background_epoch: self.background_epoch,
        });
        vec![Effect::CheckAttachments(AttachmentCheckBatch {
            id,
            purpose,
            checks: keys,
            timeout,
        })]
    }

    fn enqueue_board(&mut self, board: &SessionBoard, focused: Option<ThoughtId>, force: bool) {
        if let Some(focused) = focused {
            self.enqueue_thought_back(focused, force);
        }
        for thought in board.live_thoughts() {
            if Some(thought.id) != focused {
                self.enqueue_thought_back(thought.id, force);
            }
        }
    }

    fn enqueue_thought_front(&mut self, thought_id: ThoughtId, force: bool) {
        let keys = self.known.get(&thought_id).cloned().unwrap_or_default();
        for key in keys.into_iter().rev() {
            let current_in_flight = self.in_flight.as_ref().is_some_and(|batch| {
                batch.keys.contains(&key)
                    && (batch.purpose != AttachmentCheckPurpose::Background
                        || batch.background_epoch == self.background_epoch)
            });
            if current_in_flight || (!force && self.health.contains_key(&key)) {
                continue;
            }
            self.background.retain(|queued| queued != &key);
            self.queued.insert(key.clone());
            self.background.push_front(key);
        }
    }

    fn enqueue_thought_back(&mut self, thought_id: ThoughtId, force: bool) {
        let keys = self.known.get(&thought_id).cloned().unwrap_or_default();
        for key in keys {
            if self.should_enqueue(&key, force) {
                self.queued.insert(key.clone());
                self.background.push_back(key);
            }
        }
    }

    fn should_enqueue(&self, key: &AttachmentCheckKey, force: bool) -> bool {
        let in_flight = self.in_flight.as_ref().is_some_and(|batch| {
            batch.keys.contains(key)
                && (batch.purpose != AttachmentCheckPurpose::Background
                    || batch.background_epoch == self.background_epoch)
        });
        !self.queued.contains(key) && !in_flight && (force || !self.health.contains_key(key))
    }
}

fn take_front(queue: &mut VecDeque<AttachmentCheckKey>, limit: usize) -> Vec<AttachmentCheckKey> {
    (0..limit).filter_map(|_| queue.pop_front()).collect()
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

fn attachment_keys_by_thought(
    board: &SessionBoard,
) -> BTreeMap<ThoughtId, Vec<AttachmentCheckKey>> {
    board
        .live_thoughts()
        .into_iter()
        .map(|thought| (thought.id, attachment_keys(thought)))
        .collect()
}
