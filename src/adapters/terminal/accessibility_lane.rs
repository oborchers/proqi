//! Dedicated bounded lane for read-only attachment accessibility checks.

use std::{
    ffi::OsString,
    path::PathBuf,
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    thread::{self, JoinHandle},
};

use crate::{
    adapters::{
        attachment::worker::{decode_response, encode_request},
        process::{CancellationFlag, SystemProcessRunner},
    },
    ports::attachment_accessibility::{
        AttachmentAccessFailure, AttachmentCheckBatch, AttachmentCheckBatchResult,
        AttachmentCheckResult,
    },
    ports::environment::{ProcessError, ProcessRequest, ProcessRunner},
};

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, WorkerLifecycle, WorkerRole, join_before},
};

trait BatchExecutor: Send {
    fn execute(&mut self, batch: &AttachmentCheckBatch) -> Vec<AttachmentCheckResult>;
}

struct ProcessBatchExecutor {
    executable: PathBuf,
    runner: SystemProcessRunner,
}

/// Bounded request/result worker that never owns application policy or cache state.
pub(super) struct AccessibilityLane {
    sender: Option<SyncSender<AttachmentCheckBatch>>,
    pub(super) receiver: Receiver<AttachmentCheckBatchResult>,
    handle: Option<JoinHandle<()>>,
    lifecycle: WorkerLifecycle,
}

impl AccessibilityLane {
    pub(super) fn spawn(executable: PathBuf, cancellation: CancellationFlag) -> Self {
        Self::spawn_with(Box::new(ProcessBatchExecutor {
            executable,
            runner: SystemProcessRunner::cancellable(cancellation),
        }))
    }

    fn spawn_with(executor: Box<dyn BatchExecutor>) -> Self {
        let (request_sender, request_receiver) = sync_channel(4);
        let (result_sender, result_receiver) = sync_channel(4);
        let lifecycle = WorkerLifecycle::default();
        let worker_lifecycle = lifecycle.clone();
        let handle = thread::spawn(move || {
            worker_lifecycle.run(WorkerRole::Accessibility, || {
                accessibility_loop(&request_receiver, &result_sender, executor);
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
    mut executor: Box<dyn BatchExecutor>,
) {
    while let Ok(batch) = requests.recv() {
        let check_results = executor.execute(&batch);
        record_failures(&check_results);
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

impl BatchExecutor for ProcessBatchExecutor {
    fn execute(&mut self, batch: &AttachmentCheckBatch) -> Vec<AttachmentCheckResult> {
        let paths = batch
            .checks
            .iter()
            .map(|key| key.canonical_path.clone())
            .collect();
        let results = encode_request(paths)
            .map_err(|_| ProcessError::Io("attachment request serialization failed".to_owned()))
            .and_then(|stdin| {
                self.runner.run(ProcessRequest {
                    program: self.executable.clone().into_os_string(),
                    args: vec![OsString::from("__attachment-check")],
                    stdin: Some(stdin),
                    timeout: batch.timeout,
                })
            })
            .and_then(|output| {
                if output.exit_code != Some(0) {
                    return Err(ProcessError::Io(
                        "attachment worker exited unsuccessfully".to_owned(),
                    ));
                }
                decode_response(&output.stdout, batch.checks.len()).map_err(|()| {
                    ProcessError::Io("attachment worker returned an invalid response".to_owned())
                })
            });
        let results = match results {
            Ok(results) => results,
            Err(error) => vec![Err(process_failure(&error)); batch.checks.len()],
        };
        batch
            .checks
            .iter()
            .cloned()
            .zip(results)
            .map(|(key, result)| AttachmentCheckResult { key, result })
            .collect()
    }
}

fn process_failure(error: &ProcessError) -> AttachmentAccessFailure {
    match error {
        ProcessError::TimedOut => AttachmentAccessFailure::TimedOut,
        ProcessError::Cancelled => AttachmentAccessFailure::Cancelled,
        ProcessError::Io(_) | ProcessError::OutputLimit => AttachmentAccessFailure::Io,
    }
}

fn record_failures(results: &[AttachmentCheckResult]) {
    for result in results {
        if let Err(reason) = result.result {
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::AttachmentInaccessible {
                    reason: reason.diagnostic_code(),
                },
            );
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
    use std::{collections::VecDeque, time::Duration};

    use crate::{
        adapters::terminal::supervisor::ShutdownDeadline,
        ports::attachment_accessibility::{
            AttachmentAccessFailure, AttachmentCheckBatch, AttachmentCheckKey,
            AttachmentCheckPurpose, AttachmentCheckResult,
        },
    };

    use super::{AccessibilityLane, BatchExecutor};

    struct SequenceExecutor {
        outcomes: VecDeque<AttachmentAccessFailure>,
    }

    impl BatchExecutor for SequenceExecutor {
        fn execute(&mut self, batch: &AttachmentCheckBatch) -> Vec<AttachmentCheckResult> {
            let result = self.outcomes.pop_front().map_or(Ok(()), Err);
            batch
                .checks
                .iter()
                .cloned()
                .map(|key| AttachmentCheckResult { key, result })
                .collect()
        }
    }

    #[test]
    fn timeout_does_not_poison_the_next_bounded_batch() {
        let executor = SequenceExecutor {
            outcomes: VecDeque::from([
                AttachmentAccessFailure::TimedOut,
                AttachmentAccessFailure::Missing,
            ]),
        };
        let lane = AccessibilityLane::spawn_with(Box::new(executor));
        lane.send(batch(1, Duration::from_millis(5)))
            .expect("deadline batch");
        let timed_out = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("deadline result");
        assert_eq!(
            timed_out.results[0].result,
            Err(AttachmentAccessFailure::TimedOut)
        );
        lane.send(batch(2, Duration::from_secs(1)))
            .expect("recovery batch");
        let recovered = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("recovery result");
        assert_eq!(
            recovered.results[0].result,
            Err(AttachmentAccessFailure::Missing)
        );
        lane.stop(ShutdownDeadline::after(Duration::from_millis(150)))
            .expect("recovered lane stops within its bound");
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
