//! Transient attachment health, bounded scheduling, and submission preflight policy.

mod keys;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Duration,
};

use crate::{
    domain::{SessionBoard, SubmissionId, ThoughtId},
    ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentCheckBatch, AttachmentCheckBatchResult,
        AttachmentCheckKey, AttachmentCheckPurpose, AttachmentCheckResult,
    },
};

use super::Effect;

pub use keys::attachment_keys;
use keys::attachment_keys_by_thought;

const BACKGROUND_BATCH_SIZE: usize = 16;
const PREFLIGHT_BATCH_SIZE: usize = 32;
const BACKGROUND_TIMEOUT: Duration = Duration::from_secs(2);
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(5);
const INACTIVE_REFRESH_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentHealth {
    Unverified,
    Checking(Option<AttachmentAccessFailure>),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManualRefresh {
    epoch: u64,
    total: usize,
    completed: usize,
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

/// Why a complete board refresh was requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentRefreshCause {
    /// Quiet startup, focus, or inactivity refresh.
    Quiet,
    /// Explicit Commands action with user-visible completion.
    Manual,
}

/// Completed manual refresh for the latest exact board generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentRefreshOutcome {
    /// Number of exact current attachment annotations checked.
    pub total: usize,
    /// Number that could not be verified.
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
    manual_refresh: Option<ManualRefresh>,
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
            manual_refresh: None,
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
        self.seed_unverified();
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
        self.seed_unverified();
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
    pub fn refresh_all(
        &mut self,
        board: &SessionBoard,
        focused: Option<ThoughtId>,
        cause: AttachmentRefreshCause,
    ) -> (Vec<Effect>, Option<AttachmentRefreshOutcome>) {
        if cause == AttachmentRefreshCause::Quiet && self.manual_refresh.is_some() {
            return (Vec::new(), None);
        }
        self.background_epoch = self.background_epoch.wrapping_add(1);
        self.background.clear();
        self.queued.clear();
        self.known = attachment_keys_by_thought(board);
        self.demote_for_refresh();
        self.refreshed_since_last_deliberate = true;
        let total = self.known.values().map(Vec::len).sum();
        if cause == AttachmentRefreshCause::Manual {
            self.manual_refresh = Some(ManualRefresh {
                epoch: self.background_epoch,
                total,
                completed: 0,
                inaccessible: 0,
            });
        }
        self.enqueue_board(board, focused, true);
        let immediate = (cause == AttachmentRefreshCause::Manual && total == 0).then(|| {
            self.manual_refresh = None;
            AttachmentRefreshOutcome {
                total: 0,
                inaccessible: 0,
            }
        });
        (self.schedule(), immediate)
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
            self.refresh_all(board, focused, AttachmentRefreshCause::Quiet)
                .0
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
    ) -> (
        Vec<Effect>,
        Option<AttachmentPreflightOutcome>,
        Option<AttachmentRefreshOutcome>,
    ) {
        let AttachmentCheckBatchResult {
            id,
            purpose,
            results,
        } = completion;
        let Some(in_flight) = self.in_flight.take() else {
            return (Vec::new(), None, None);
        };
        if id != in_flight.id || purpose != in_flight.purpose {
            self.in_flight = Some(in_flight);
            return (Vec::new(), None, None);
        }
        let failures = self.apply_results(&in_flight, &results);
        let (preflight, refresh) = match in_flight.purpose {
            AttachmentCheckPurpose::Background => {
                (None, self.finish_manual_refresh_batch(&in_flight, failures))
            }
            AttachmentCheckPurpose::SubmissionPreflight(submission_id) => {
                (self.finish_preflight_batch(submission_id, failures), None)
            }
        };
        let effects = self.schedule();
        (effects, preflight, refresh)
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
            .is_none_or(|health| !matches!(health, AttachmentHealth::Accessible))
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

    fn finish_manual_refresh_batch(
        &mut self,
        in_flight: &InFlight,
        failures: usize,
    ) -> Option<AttachmentRefreshOutcome> {
        let refresh = self.manual_refresh.as_mut()?;
        if refresh.epoch != in_flight.background_epoch {
            return None;
        }
        refresh.completed = refresh.completed.saturating_add(in_flight.keys.len());
        refresh.inaccessible = refresh.inaccessible.saturating_add(failures);
        if refresh.completed < refresh.total {
            return None;
        }
        let outcome = AttachmentRefreshOutcome {
            total: refresh.total,
            inaccessible: refresh.inaccessible,
        };
        self.manual_refresh = None;
        Some(outcome)
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
        for key in &keys {
            let previous = match self.health.get(key) {
                Some(
                    AttachmentHealth::Inaccessible(failure)
                    | AttachmentHealth::Checking(Some(failure)),
                ) => Some(*failure),
                _ => None,
            };
            self.health
                .insert(key.clone(), AttachmentHealth::Checking(previous));
        }
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
            if current_in_flight || (!force && self.has_result(&key)) {
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
        !self.queued.contains(key) && !in_flight && (force || !self.has_result(key))
    }
}

fn take_front(queue: &mut VecDeque<AttachmentCheckKey>, limit: usize) -> Vec<AttachmentCheckKey> {
    (0..limit).filter_map(|_| queue.pop_front()).collect()
}
