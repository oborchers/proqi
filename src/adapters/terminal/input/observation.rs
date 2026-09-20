//! Reader-stage evidence and the existing supervisor lease policy.

use super::SOURCE_STALL_LIMIT;
use crate::adapters::diagnostics::{
    InputObservation, InputReaderStage, InputStallEvidence, record_input_observation,
};
use std::time::Duration;
use std::{sync::Mutex, time::Instant};

#[derive(Debug, Eq, PartialEq)]
pub(super) enum LeaseDecision {
    Continue,
    ResetAfterSupervisorGap { gap: Duration },
    Unresponsive,
}

pub(super) struct SourceLease {
    last_response: Instant,
    last_observation: Instant,
}

impl SourceLease {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            last_response: now,
            last_observation: now,
        }
    }

    pub(super) fn observe(&mut self, now: Instant, reader_responded: bool) -> LeaseDecision {
        let supervisor_gap = now.saturating_duration_since(self.last_observation);
        self.last_observation = now;
        if reader_responded {
            self.last_response = now;
            return LeaseDecision::Continue;
        }
        if supervisor_gap >= SOURCE_STALL_LIMIT {
            self.last_response = now;
            return LeaseDecision::ResetAfterSupervisorGap {
                gap: supervisor_gap,
            };
        }
        if now.saturating_duration_since(self.last_response) >= SOURCE_STALL_LIMIT {
            LeaseDecision::Unresponsive
        } else {
            LeaseDecision::Continue
        }
    }
}

#[derive(Clone, Copy)]
struct Progress {
    stage: InputReaderStage,
    entered: Instant,
    completed: Option<(InputReaderStage, Instant)>,
}

pub(super) struct ReaderProgress(Mutex<Progress>);

impl ReaderProgress {
    pub(super) fn new(now: Instant) -> Self {
        Self(Mutex::new(Progress {
            stage: InputReaderStage::Starting,
            entered: now,
            completed: None,
        }))
    }

    pub(super) fn enter(&self, stage: InputReaderStage, now: Instant) {
        if let Ok(mut progress) = self.0.lock() {
            progress.stage = stage;
            progress.entered = now;
        }
    }

    pub(super) fn complete(&self, now: Instant) {
        if let Ok(mut progress) = self.0.lock() {
            progress.completed = Some((progress.stage, now));
        }
    }
}

impl SourceLease {
    pub(super) fn evidence(&self, now: Instant, reader: &ReaderProgress) -> InputStallEvidence {
        // Never wait for instrumentation while supervising the real input owner.
        // A contended/poisoned snapshot is explicitly absent, not guessed.
        let progress = reader.0.try_lock().ok().map(|progress| *progress);
        InputStallEvidence {
            reader_stage: progress.map(|value| value.stage),
            reader_stage_elapsed_ms: progress.map(|value| elapsed_ms(now, value.entered)),
            last_completed_stage: progress
                .and_then(|value| value.completed.map(|(stage, _)| stage)),
            last_completed_gap_ms: progress
                .and_then(|value| value.completed.map(|(_, at)| elapsed_ms(now, at))),
            lease_gap_ms: elapsed_ms(now, self.last_response),
            observer_gap_ms: elapsed_ms(now, self.last_observation),
        }
    }
}

fn elapsed_ms(now: Instant, earlier: Instant) -> u64 {
    u64::try_from(now.saturating_duration_since(earlier).as_millis()).unwrap_or(u64::MAX)
}

pub(super) fn record_supervisor_gap(evidence: InputStallEvidence) {
    record_input_observation(InputObservation::SupervisorGap, evidence);
}

pub(super) fn record_stall(evidence: InputStallEvidence) {
    record_input_observation(InputObservation::ConfirmedStall, evidence);
}

#[cfg(test)]
mod tests;
