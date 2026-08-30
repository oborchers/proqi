//! Dedicated bounded lane for read-only attachment accessibility checks.

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    adapters::{attachment::FileAttachmentAccessibility, process::CancellationFlag},
    ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentAccessibility, AttachmentCheckBatch,
        AttachmentCheckBatchResult, AttachmentCheckResult,
    },
};

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, WorkerLifecycle, WorkerRole, join_before},
};

const CHECK_CANCELLATION_INTERVAL: Duration = Duration::from_millis(25);
const CHECKER_JOIN_GRACE: Duration = Duration::from_millis(50);

struct CheckRequest {
    id: u64,
    path: PathBuf,
}

struct CheckResponse {
    id: u64,
    result: Result<(), AttachmentAccessFailure>,
}

/// Bounded request/result worker that never owns application policy or cache state.
pub(super) struct AccessibilityLane {
    sender: Option<SyncSender<AttachmentCheckBatch>>,
    pub(super) receiver: Receiver<AttachmentCheckBatchResult>,
    handle: Option<JoinHandle<()>>,
    lifecycle: WorkerLifecycle,
}

impl AccessibilityLane {
    pub(super) fn spawn(cancellation: CancellationFlag) -> Self {
        Self::spawn_with(Box::<FileAttachmentAccessibility>::default(), cancellation)
    }

    fn spawn_with(
        checker: Box<dyn AttachmentAccessibility>,
        cancellation: CancellationFlag,
    ) -> Self {
        let (request_sender, request_receiver) = sync_channel(4);
        let (result_sender, result_receiver) = sync_channel(4);
        let lifecycle = WorkerLifecycle::default();
        let worker_lifecycle = lifecycle.clone();
        let handle = thread::spawn(move || {
            worker_lifecycle.run(WorkerRole::Accessibility, || {
                accessibility_loop(&request_receiver, &result_sender, checker, &cancellation);
            });
        });
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
            lifecycle,
        }
    }

    pub(super) fn send(&self, request: AttachmentCheckBatch) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("accessibility lane is closed"))?
            .try_send(request)
            .map_err(|error| map_send_error(&error))
    }

    pub(super) fn request_stop(&mut self) {
        self.lifecycle.request_stop();
        self.sender = None;
    }

    pub(super) fn worker_failure(&self) -> Option<TerminalError> {
        self.lifecycle.failure(WorkerRole::Accessibility)
    }

    pub(super) fn stopped_cleanly(&self) -> bool {
        self.lifecycle.stopped_cleanly()
    }

    pub(super) fn stop(mut self, deadline: ShutdownDeadline) -> Result<(), TerminalError> {
        self.request_stop();
        while !deadline.expired() {
            match self.receiver.recv_timeout(deadline.remaining()) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout) => break,
            }
        }
        join_before(
            self.handle.take(),
            deadline,
            "accessibility lane panicked",
            "accessibility lane did not stop before the shutdown deadline",
        )
    }
}

impl Drop for AccessibilityLane {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn accessibility_loop(
    requests: &Receiver<AttachmentCheckBatch>,
    results: &SyncSender<AttachmentCheckBatchResult>,
    checker: Box<dyn AttachmentAccessibility>,
    cancellation: &CancellationFlag,
) {
    let (check_sender, check_receiver) = sync_channel(1);
    let (response_sender, response_receiver) = sync_channel(2);
    let checker_handle = thread::spawn(move || {
        checker_loop(&check_receiver, &response_sender, checker);
    });
    let mut next_check_id = 1_u64;
    while let Ok(batch) = requests.recv() {
        let deadline = Instant::now() + batch.timeout;
        let check_results = batch
            .checks
            .iter()
            .cloned()
            .map(|key| {
                let result = check_one(
                    &check_sender,
                    &response_receiver,
                    &key,
                    next_check_id,
                    deadline,
                    cancellation,
                );
                next_check_id = next_check_id.wrapping_add(1).max(1);
                if let Err(reason) = result {
                    crate::adapters::diagnostics::record(
                        crate::adapters::diagnostics::SafeEvent::AttachmentInaccessible {
                            reason: reason.diagnostic_code(),
                        },
                    );
                }
                AttachmentCheckResult { key, result }
            })
            .collect();
        let completion = AttachmentCheckBatchResult {
            id: batch.id,
            purpose: batch.purpose,
            results: check_results,
        };
        if results.send(completion).is_err() {
            return;
        }
    }
    drop(check_sender);
    let _joined = join_before(
        Some(checker_handle),
        ShutdownDeadline::after(CHECKER_JOIN_GRACE),
        "attachment checker panicked",
        "attachment checker did not stop",
    );
}

fn check_one(
    requests: &SyncSender<CheckRequest>,
    responses: &Receiver<CheckResponse>,
    key: &crate::ports::attachment_accessibility::AttachmentCheckKey,
    id: u64,
    deadline: Instant,
    cancellation: &CancellationFlag,
) -> Result<(), AttachmentAccessFailure> {
    if cancellation.is_cancelled() {
        return Err(AttachmentAccessFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AttachmentAccessFailure::TimedOut);
    }
    requests
        .try_send(CheckRequest {
            id,
            path: key.path().to_path_buf(),
        })
        .map_err(|error| match error {
            TrySendError::Full(_) => AttachmentAccessFailure::TimedOut,
            TrySendError::Disconnected(_) => AttachmentAccessFailure::Io,
        })?;
    loop {
        if cancellation.is_cancelled() {
            return Err(AttachmentAccessFailure::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AttachmentAccessFailure::TimedOut);
        }
        match responses.recv_timeout(remaining.min(CHECK_CANCELLATION_INTERVAL)) {
            Ok(response) if response.id == id => return response.result,
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(AttachmentAccessFailure::Io),
        }
    }
}

fn checker_loop(
    requests: &Receiver<CheckRequest>,
    responses: &SyncSender<CheckResponse>,
    mut checker: Box<dyn AttachmentAccessibility>,
) {
    while let Ok(request) = requests.recv() {
        let response = CheckResponse {
            id: request.id,
            result: checker.check(&request.path),
        };
        if responses.send(response).is_err() {
            return;
        }
    }
}

fn map_send_error(error: &TrySendError<AttachmentCheckBatch>) -> TerminalError {
    match error {
        TrySendError::Full(_) => TerminalError::Worker("accessibility lane is full"),
        TrySendError::Disconnected(_) => TerminalError::Worker("accessibility lane disconnected"),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::mpsc::{Receiver, Sender, channel},
        time::Duration,
    };

    use crate::{
        adapters::process::CancellationFlag,
        adapters::terminal::supervisor::ShutdownDeadline,
        ports::attachment_accessibility::{
            AttachmentAccessFailure, AttachmentAccessibility, AttachmentCheckBatch,
            AttachmentCheckKey, AttachmentCheckPurpose,
        },
    };

    use super::AccessibilityLane;

    struct Blocking {
        started: Sender<()>,
        release: Receiver<()>,
    }

    impl AttachmentAccessibility for Blocking {
        fn check(&mut self, _path: &Path) -> Result<(), AttachmentAccessFailure> {
            self.started
                .send(())
                .map_err(|_| AttachmentAccessFailure::Io)?;
            self.release.recv().map_err(|_| AttachmentAccessFailure::Io)
        }
    }

    #[test]
    fn deadline_and_cancellation_fail_closed() {
        let cancellation = CancellationFlag::default();
        let (checker, started, release) = blocking();
        let lane = AccessibilityLane::spawn_with(Box::new(checker), cancellation.clone());
        lane.send(batch(1, Duration::from_millis(5)))
            .expect("deadline batch");
        started
            .recv_timeout(Duration::from_secs(1))
            .expect("checker started");
        let timed_out = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("deadline result");
        assert_eq!(
            timed_out.results[0].result,
            Err(AttachmentAccessFailure::TimedOut)
        );
        release.send(()).expect("release timed out checker");

        lane.stop(ShutdownDeadline::after(Duration::from_millis(150)))
            .expect("deadline lane stops within its bound");

        let cancellation = CancellationFlag::default();
        let (checker, started, release) = blocking();
        let lane = AccessibilityLane::spawn_with(Box::new(checker), cancellation.clone());
        lane.send(batch(2, Duration::from_secs(1)))
            .expect("cancelled batch");
        started
            .recv_timeout(Duration::from_secs(1))
            .expect("checker started");
        cancellation.cancel();
        let cancelled = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("cancelled result");
        assert_eq!(
            cancelled.results[0].result,
            Err(AttachmentAccessFailure::Cancelled)
        );
        release.send(()).expect("release cancelled checker");
        lane.stop(ShutdownDeadline::after(Duration::from_millis(150)))
            .expect("cancelled lane stops within its bound");
    }

    fn blocking() -> (Blocking, Receiver<()>, Sender<()>) {
        let (started_sender, started_receiver) = channel();
        let (release_sender, release_receiver) = channel();
        (
            Blocking {
                started: started_sender,
                release: release_receiver,
            },
            started_receiver,
            release_sender,
        )
    }

    fn batch(id: u64, timeout: Duration) -> AttachmentCheckBatch {
        AttachmentCheckBatch {
            id,
            purpose: AttachmentCheckPurpose::Background,
            checks: vec![AttachmentCheckKey {
                thought_id: "tht_06g30t7dv5qv55n1ppn3clis3k"
                    .parse()
                    .expect("thought id"),
                annotation_index: 0,
                annotation_start: 0,
                annotation_end: 10,
                image: false,
                display_name: "fixture".to_owned(),
                canonical_path: "/tmp/fixture".to_owned(),
                content_revision: [1; 32],
            }],
            timeout,
        }
    }
}
