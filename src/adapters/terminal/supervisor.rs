//! Bounded, shared shutdown timing for terminal worker lanes.

use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::TerminalError;

pub(super) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

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
pub(super) struct ShutdownDeadline(Instant);

impl ShutdownDeadline {
    pub(super) fn after(timeout: Duration) -> Self {
        Self(Instant::now() + timeout)
    }

    pub(super) fn remaining(self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    pub(super) fn expired(self) -> bool {
        self.remaining().is_zero()
    }

    pub(super) const fn instant(self) -> Instant {
        self.0
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

    use super::{ShutdownCoordinator, ShutdownDeadline, join_before};

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
