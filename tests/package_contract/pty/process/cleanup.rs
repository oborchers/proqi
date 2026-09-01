//! Persistent state for one bounded PTY teardown attempt.

use std::time::{Duration, Instant};

use rustix::{
    io::Errno,
    process::{Pid, test_kill_process_group},
};

use super::PtyChild;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CleanupOutcome {
    pub(super) child: CleanupState,
    pub(super) group: CleanupState,
    pub(super) reader: CleanupState,
}

impl CleanupOutcome {
    pub(super) fn new(child: &PtyChild) -> Self {
        Self {
            child: CleanupState::from_complete(child.child.is_none()),
            group: CleanupState::from_complete(child.process_group.is_none()),
            reader: CleanupState::from_complete(child.reader.is_none()),
        }
    }

    pub(super) const fn processes_gone(self) -> bool {
        matches!(self.child, CleanupState::Complete) && matches!(self.group, CleanupState::Complete)
    }

    pub(super) const fn settled(self) -> bool {
        self.processes_gone() && !matches!(self.reader, CleanupState::Pending)
    }

    pub(super) const fn successful(self) -> bool {
        self.processes_gone() && matches!(self.reader, CleanupState::Complete)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CleanupState {
    Pending,
    Complete,
    Failed,
}

impl CleanupState {
    const fn from_complete(complete: bool) -> Self {
        if complete {
            Self::Complete
        } else {
            Self::Pending
        }
    }
}

pub(super) struct CleanupProgress {
    pub(super) deadline: TeardownDeadline,
    pub(super) force_at: TeardownDeadline,
    pub(super) term_sent: bool,
    pub(super) kill_sent: bool,
    pub(super) outcome: CleanupOutcome,
}

impl CleanupProgress {
    pub(super) fn next_deadline(&self) -> TeardownDeadline {
        if self.kill_sent || self.outcome.processes_gone() {
            self.deadline
        } else {
            self.force_at
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct TeardownDeadline(Instant);

impl TeardownDeadline {
    pub(super) fn after(timeout: Duration) -> Self {
        Self(Instant::now() + timeout)
    }

    pub(super) fn capped_after(self, timeout: Duration) -> Self {
        Self(self.0.min(Instant::now() + timeout))
    }

    pub(super) fn remaining(self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    pub(super) fn expired(self) -> bool {
        self.remaining().is_zero()
    }
}

pub(super) fn process_group_is_absent(group: Pid) -> bool {
    matches!(test_kill_process_group(group), Err(Errno::SRCH))
}
