//! Bounded, shared shutdown timing for terminal worker lanes.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::TerminalError;

pub(super) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

const RUNNING: u8 = 0;
const STOPPING: u8 = 1;
const EXITED: u8 = 2;
const UNEXPECTED_EXIT: u8 = 3;
const PANICKED: u8 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkerRole {
    Accessibility,
    Persistence,
    External,
    Update,
    Screenshot,
}

impl WorkerRole {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::Persistence => "persistence",
            Self::External => "external",
            Self::Update => "update",
            Self::Screenshot => "screenshot",
        }
    }

    const fn panicked_message(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility lane panicked",
            Self::Persistence => "persistence lane panicked",
            Self::External => "external lane panicked",
            Self::Update => "update lane panicked",
            Self::Screenshot => "screenshot lane panicked",
        }
    }

    const fn exited_message(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility lane exited unexpectedly",
            Self::Persistence => "persistence lane exited unexpectedly",
            Self::External => "external lane exited unexpectedly",
            Self::Update => "update lane exited unexpectedly",
            Self::Screenshot => "screenshot lane exited unexpectedly",
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct WorkerLifecycle(Arc<AtomicU8>);

impl WorkerLifecycle {
    pub(super) fn request_stop(&self) {
        let _previous =
            self.0
                .compare_exchange(RUNNING, STOPPING, Ordering::AcqRel, Ordering::Acquire);
    }

    pub(super) fn run(&self, role: WorkerRole, work: impl FnOnce()) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        if outcome.is_err() {
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::RuntimePanicked {
                    role: role.as_str(),
                },
            );
            self.0.store(PANICKED, Ordering::Release);
            return;
        }
        let next = if self.0.load(Ordering::Acquire) == STOPPING {
            EXITED
        } else {
            UNEXPECTED_EXIT
        };
        self.0.store(next, Ordering::Release);
    }

    pub(super) fn failure(&self, role: WorkerRole) -> Option<TerminalError> {
        match self.0.load(Ordering::Acquire) {
            PANICKED => Some(TerminalError::Worker(role.panicked_message())),
            UNEXPECTED_EXIT => Some(TerminalError::Worker(role.exited_message())),
            _ => None,
        }
    }

    pub(super) fn stopped_cleanly(&self) -> bool {
        matches!(self.0.load(Ordering::Acquire), STOPPING | EXITED)
    }
}

#[derive(Clone, Default)]
pub(super) struct ShutdownCoordinator(Arc<Mutex<Option<ShutdownDeadline>>>);

impl ShutdownCoordinator {
    pub(super) fn request(&self) -> ShutdownDeadline {
        let mut deadline = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *deadline.get_or_insert_with(|| ShutdownDeadline::after(SHUTDOWN_TIMEOUT))
    }
}

#[derive(Clone, Copy)]
pub(super) struct ShutdownDeadline {
    started: Instant,
    deadline: Instant,
}

impl ShutdownDeadline {
    pub(super) fn after(timeout: Duration) -> Self {
        let started = Instant::now();
        Self {
            started,
            deadline: started + timeout,
        }
    }

    pub(super) fn remaining(self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub(super) fn expired(self) -> bool {
        self.remaining().is_zero()
    }

    pub(super) const fn instant(self) -> Instant {
        self.deadline
    }

    pub(super) fn elapsed(self) -> Duration {
        self.started.elapsed()
    }
}

pub(super) fn join_before(
    mut handle: Option<JoinHandle<()>>,
    deadline: ShutdownDeadline,
    panicked: &'static str,
    timed_out: &'static str,
) -> Result<(), TerminalError> {
    while handle.as_ref().is_some_and(|worker| !worker.is_finished()) && !deadline.expired() {
        std::thread::sleep(Duration::from_millis(2));
    }
    let Some(worker) = handle.take() else {
        return Ok(());
    };
    if !worker.is_finished() {
        return Err(TerminalError::Worker(timed_out));
    }
    worker.join().map_err(|_| TerminalError::Worker(panicked))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{ShutdownCoordinator, ShutdownDeadline, WorkerLifecycle, WorkerRole, join_before};

    #[test]
    fn worker_exit_is_clean_only_after_stop_was_requested() {
        let unexpected = WorkerLifecycle::default();
        unexpected.run(WorkerRole::Update, || {});
        assert!(unexpected.failure(WorkerRole::Update).is_some());

        let clean = WorkerLifecycle::default();
        clean.request_stop();
        clean.run(WorkerRole::Update, || {});
        assert!(clean.failure(WorkerRole::Update).is_none());
        assert!(clean.stopped_cleanly());
    }

    #[test]
    fn worker_panic_is_observable_without_propagating_the_payload() {
        let lifecycle = WorkerLifecycle::default();
        lifecycle.run(WorkerRole::External, || panic!("secret panic payload"));
        assert_eq!(
            lifecycle
                .failure(WorkerRole::External)
                .map(|error| error.to_string()),
            Some("terminal worker failed: external lane panicked".to_owned())
        );
    }

    #[test]
    fn repeated_shutdown_requests_share_the_original_deadline() {
        let shutdown = ShutdownCoordinator::default();
        let first = shutdown.request().instant();
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(shutdown.request().instant(), first);
    }

    #[test]
    fn nonresponsive_worker_never_blocks_shutdown_past_the_deadline() {
        let handle = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(200)));
        let started = Instant::now();
        let result = join_before(
            Some(handle),
            ShutdownDeadline::after(Duration::from_millis(10)),
            "worker panicked",
            "worker timed out",
        );
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_millis(100));
    }
}
