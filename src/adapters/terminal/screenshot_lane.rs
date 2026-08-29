//! Bounded screenshot watcher worker and installation-wide ownership acquisition.

use std::{
    sync::{
        Arc,
        mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    adapters::{
        runtime::{FileCaptureLease, FileRuntimeCoordinator},
        screenshot::SystemScreenshotWatcherFactory,
    },
    ports::{
        runtime::{CaptureCoordinator, CaptureLockError, InstanceInfo},
        screenshot::{
            ActiveScreenshotWatcher, ScreenshotCandidate, ScreenshotError, ScreenshotWatcherFactory,
        },
    },
};

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, WorkerLifecycle, WorkerRole, join_before},
};

pub(super) enum ScreenshotResult {
    Started(FileCaptureLease),
    Candidates(Vec<ScreenshotCandidate>),
    Conflict(Option<Box<crate::ports::runtime::CaptureOwnerInfo>>),
    Stopped(Vec<ScreenshotCandidate>),
    Failed {
        error: ScreenshotError,
        release_when_drained: bool,
    },
}

enum ScreenshotRequest {
    Enable,
    Disable,
    TakeOver {
        owner: crate::ports::runtime::CaptureOwnerInfo,
        request_id: crate::domain::RequestId,
    },
}

pub(super) struct ScreenshotLane {
    sender: Option<SyncSender<ScreenshotRequest>>,
    pub(super) receiver: Receiver<ScreenshotResult>,
    handle: Option<JoinHandle<()>>,
    lifecycle: WorkerLifecycle,
    cancellation: crate::adapters::process::CancellationFlag,
}

struct ScreenshotWorker {
    coordinator: FileRuntimeCoordinator,
    factory: Arc<dyn ScreenshotWatcherFactory>,
    instance: InstanceInfo,
    settings: super::settings::ScreenshotSettings,
    terminal_host: String,
    watcher: Option<Box<dyn ActiveScreenshotWatcher>>,
    cancellation: crate::adapters::process::CancellationFlag,
}

impl ScreenshotLane {
    pub(super) fn spawn(
        coordinator: FileRuntimeCoordinator,
        instance: InstanceInfo,
        settings: super::settings::ScreenshotSettings,
        terminal_host: String,
    ) -> Self {
        Self::spawn_with_factory(
            coordinator,
            instance,
            settings,
            terminal_host,
            Arc::new(SystemScreenshotWatcherFactory),
        )
    }

    fn spawn_with_factory(
        coordinator: FileRuntimeCoordinator,
        instance: InstanceInfo,
        settings: super::settings::ScreenshotSettings,
        terminal_host: String,
        factory: Arc<dyn ScreenshotWatcherFactory>,
    ) -> Self {
        let (sender, requests) = sync_channel(8);
        let (results, receiver) = sync_channel(64);
        let lifecycle = WorkerLifecycle::default();
        let cancellation = crate::adapters::process::CancellationFlag::default();
        let worker_cancellation = cancellation.clone();
        let worker_lifecycle = lifecycle.clone();
        let handle = thread::spawn(move || {
            worker_lifecycle.run(WorkerRole::Screenshot, || {
                ScreenshotWorker {
                    coordinator,
                    factory,
                    instance,
                    settings,
                    terminal_host,
                    watcher: None,
                    cancellation: worker_cancellation,
                }
                .run(&requests, &results);
            });
        });
        Self {
            sender: Some(sender),
            receiver,
            handle: Some(handle),
            lifecycle,
            cancellation,
        }
    }

    pub(super) fn enable(&self) -> Result<(), TerminalError> {
        self.send(ScreenshotRequest::Enable)
    }

    pub(super) fn disable(&self) -> Result<(), TerminalError> {
        self.send(ScreenshotRequest::Disable)
    }

    pub(super) fn take_over(
        &self,
        owner: crate::ports::runtime::CaptureOwnerInfo,
        request_id: crate::domain::RequestId,
    ) -> Result<(), TerminalError> {
        self.send(ScreenshotRequest::TakeOver { owner, request_id })
    }

    fn send(&self, request: ScreenshotRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("screenshot lane is closed"))?
            .try_send(request)
            .map_err(|error| match error {
                TrySendError::Full(_) => TerminalError::Worker("screenshot lane is full"),
                TrySendError::Disconnected(_) => {
                    TerminalError::Worker("screenshot lane disconnected")
                }
            })
    }

    pub(super) fn request_stop(&mut self) {
        self.lifecycle.request_stop();
        self.cancellation.cancel();
        self.sender = None;
    }

    pub(super) fn stopped_cleanly(&self) -> bool {
        self.lifecycle.stopped_cleanly()
    }

    pub(super) fn worker_failure(&self) -> Option<TerminalError> {
        self.lifecycle.failure(WorkerRole::Screenshot)
    }

    pub(super) fn stop(mut self, deadline: ShutdownDeadline) -> Result<(), TerminalError> {
        self.request_stop();
        join_before(
            self.handle.take(),
            deadline,
            "screenshot lane panicked",
            "screenshot lane did not stop before the shutdown deadline",
        )
    }
}

impl Drop for ScreenshotLane {
    fn drop(&mut self) {
        self.request_stop();
    }
}

impl ScreenshotWorker {
    fn run(
        mut self,
        requests: &Receiver<ScreenshotRequest>,
        results: &SyncSender<ScreenshotResult>,
    ) {
        loop {
            let request = if self.watcher.is_some() {
                match requests.recv_timeout(Duration::from_millis(25)) {
                    Ok(request) => Some(request),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match requests.recv() {
                    Ok(request) => Some(request),
                    Err(_) => break,
                }
            };
            if let Some(request) = request
                && !self.handle_request(request, results)
            {
                return;
            }
            if !self.poll_watcher(results) {
                return;
            }
        }
        self.finish(results);
    }

    fn poll_watcher(&mut self, results: &SyncSender<ScreenshotResult>) -> bool {
        let Some(watcher) = self.watcher.as_mut() else {
            return true;
        };
        match watcher.poll() {
            Ok(candidates) if candidates.is_empty() => true,
            Ok(candidates) => self.send_result(results, ScreenshotResult::Candidates(candidates)),
            Err(error) => {
                self.watcher = None;
                self.send_result(
                    results,
                    ScreenshotResult::Failed {
                        error,
                        release_when_drained: false,
                    },
                )
            }
        }
    }

    fn handle_request(
        &mut self,
        request: ScreenshotRequest,
        results: &SyncSender<ScreenshotResult>,
    ) -> bool {
        match request {
            ScreenshotRequest::Enable if self.watcher.is_none() => self.enable(results),
            ScreenshotRequest::Enable => true,
            ScreenshotRequest::Disable => self.disable(results),
            ScreenshotRequest::TakeOver { owner, request_id } => {
                self.take_over(results, &owner, request_id)
            }
        }
    }

    fn enable(&mut self, results: &SyncSender<ScreenshotResult>) -> bool {
        let config = match self.settings.watcher_config() {
            Ok(config) => config,
            Err(error) => return self.fail(results, error, false),
        };
        let lease = match self.coordinator.acquire_capture(&self.instance) {
            Ok(lease) => lease,
            Err(CaptureLockError::Busy { owner }) => {
                return self.send_result(results, ScreenshotResult::Conflict(owner));
            }
            Err(CaptureLockError::ControlUnavailable) => {
                return self.fail(results, ScreenshotError::ControlUnavailable, false);
            }
            Err(_) => return self.fail(results, ScreenshotError::Ownership, false),
        };
        self.start_owned(results, config, lease)
    }

    fn disable(&mut self, results: &SyncSender<ScreenshotResult>) -> bool {
        match self.reconcile() {
            Ok(candidates) => self.send_result(results, ScreenshotResult::Stopped(candidates)),
            Err(error) => self.fail(results, error, true),
        }
    }

    fn take_over(
        &mut self,
        results: &SyncSender<ScreenshotResult>,
        owner: &crate::ports::runtime::CaptureOwnerInfo,
        request_id: crate::domain::RequestId,
    ) -> bool {
        let config = match self.settings.watcher_config() {
            Ok(config) => config,
            Err(error) => return self.fail(results, error, false),
        };
        let client =
            crate::adapters::control::CancellableLocalControlClient::new(self.cancellation.clone());
        if client
            .request_capture_takeover(owner, self.instance.instance_id, request_id)
            .is_err()
        {
            return self.fail(results, ScreenshotError::TakeoverFailed, false);
        }
        let Some(lease) = self.await_released_capture() else {
            return self.fail(results, ScreenshotError::TakeoverFailed, false);
        };
        self.start_owned(results, config, lease)
    }

    fn await_released_capture(&self) -> Option<FileCaptureLease> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline && !self.cancellation.is_cancelled() {
            match self.coordinator.acquire_capture(&self.instance) {
                Ok(lease) => return Some(lease),
                Err(CaptureLockError::Busy { .. }) => thread::sleep(Duration::from_millis(25)),
                Err(_) => return None,
            }
        }
        None
    }

    fn start_owned(
        &mut self,
        results: &SyncSender<ScreenshotResult>,
        config: crate::ports::screenshot::ScreenshotInboxConfig,
        lease: FileCaptureLease,
    ) -> bool {
        match self.factory.start(
            config,
            &self.terminal_host,
            Arc::new(self.cancellation.clone()),
        ) {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.send_result(results, ScreenshotResult::Started(lease))
            }
            Err(error) => {
                drop(lease);
                self.fail(results, error, false)
            }
        }
    }

    fn fail(
        &self,
        results: &SyncSender<ScreenshotResult>,
        error: ScreenshotError,
        release_when_drained: bool,
    ) -> bool {
        self.send_result(
            results,
            ScreenshotResult::Failed {
                error,
                release_when_drained,
            },
        )
    }

    fn reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        let candidates = self
            .watcher
            .as_mut()
            .map_or(Ok(Vec::new()), |watcher| watcher.final_reconcile());
        self.watcher = None;
        candidates
    }

    fn finish(&mut self, results: &SyncSender<ScreenshotResult>) {
        if self.watcher.is_some() {
            let result = match self.reconcile() {
                Ok(candidates) => ScreenshotResult::Stopped(candidates),
                Err(error) => ScreenshotResult::Failed {
                    error,
                    release_when_drained: true,
                },
            };
            let _sent = self.send_result(results, result);
        }
    }

    fn send_result(
        &self,
        results: &SyncSender<ScreenshotResult>,
        mut result: ScreenshotResult,
    ) -> bool {
        loop {
            match results.try_send(result) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(returned)) if self.cancellation.is_cancelled() => {
                    drop(returned);
                    return false;
                }
                Err(TrySendError::Full(returned)) => {
                    result = returned;
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "screenshot_lane/tests.rs"]
mod tests;
