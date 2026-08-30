//! Dedicated bounded lane for read-only attachment accessibility checks.

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::Instant,
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
    mut checker: Box<dyn AttachmentAccessibility>,
    cancellation: &CancellationFlag,
) {
    while let Ok(batch) = requests.recv() {
        let deadline = Instant::now() + batch.timeout;
        let check_results = batch
            .checks
            .iter()
            .cloned()
            .map(|key| {
                let result = check_one(checker.as_mut(), &key, deadline, cancellation);
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
}

fn check_one(
    checker: &mut dyn AttachmentAccessibility,
    key: &crate::ports::attachment_accessibility::AttachmentCheckKey,
    deadline: Instant,
    cancellation: &CancellationFlag,
) -> Result<(), AttachmentAccessFailure> {
    if cancellation.is_cancelled() {
        return Err(AttachmentAccessFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AttachmentAccessFailure::TimedOut);
    }
    let result = checker.check(key.path());
    if cancellation.is_cancelled() {
        Err(AttachmentAccessFailure::Cancelled)
    } else if Instant::now() >= deadline {
        Err(AttachmentAccessFailure::TimedOut)
    } else {
        result
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
    use std::{path::Path, thread, time::Duration};

    use crate::{
        adapters::process::CancellationFlag,
        ports::attachment_accessibility::{
            AttachmentAccessFailure, AttachmentAccessibility, AttachmentCheckBatch,
            AttachmentCheckKey, AttachmentCheckPurpose,
        },
    };

    use super::AccessibilityLane;

    struct Slow;

    impl AttachmentAccessibility for Slow {
        fn check(&mut self, _path: &Path) -> Result<(), AttachmentAccessFailure> {
            thread::sleep(Duration::from_millis(5));
            Ok(())
        }
    }

    #[test]
    fn deadline_and_cancellation_fail_closed() {
        let cancellation = CancellationFlag::default();
        let mut lane = AccessibilityLane::spawn_with(Box::new(Slow), cancellation.clone());
        lane.send(batch(1, Duration::from_millis(1)))
            .expect("deadline batch");
        let timed_out = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("deadline result");
        assert_eq!(
            timed_out.results[0].result,
            Err(AttachmentAccessFailure::TimedOut)
        );

        cancellation.cancel();
        lane.send(batch(2, Duration::from_secs(1)))
            .expect("cancelled batch");
        let cancelled = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("cancelled result");
        assert_eq!(
            cancelled.results[0].result,
            Err(AttachmentAccessFailure::Cancelled)
        );
        lane.request_stop();
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
