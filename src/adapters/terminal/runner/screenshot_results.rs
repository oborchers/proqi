//! Screenshot watcher results applied on the owner UI lane.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::terminal::{TerminalError, screenshot_lane::ScreenshotResult},
    ports::screenshot::ScreenshotError,
    ui::BoardApp,
};

use super::{
    CaptureRuntime, PendingWork, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
};

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
) -> Result<DrainOutcome, TerminalError> {
    drain_bounded(
        || match lanes.screenshot.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if lanes.screenshot.stopped_cleanly() => Ok(None),
            Err(TryRecvError::Disconnected) => Err(lanes
                .screenshot
                .worker_failure()
                .unwrap_or(TerminalError::Worker("screenshot result lane disconnected"))),
        },
        |result| {
            match result {
                ScreenshotResult::Started(lease) => {
                    pending.screenshot = pending.screenshot.saturating_sub(1);
                    if capture.lease.replace(lease).is_some() {
                        return Err(TerminalError::Worker(
                            "screenshot lane returned overlapping capture leases",
                        ));
                    }
                    capture.takeover_stopping = false;
                    app.screenshot_started();
                }
                ScreenshotResult::Candidates(candidates) => {
                    app.queue_screenshot_candidates(candidates);
                }
                ScreenshotResult::Conflict(Some(owner)) => {
                    pending.screenshot = pending.screenshot.saturating_sub(1);
                    if owner.capture_protocol
                        == crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION
                        && owner.control_protocol == crate::ports::control::CONTROL_PROTOCOL_VERSION
                    {
                        app.screenshot_conflict(*owner);
                    } else {
                        app.screenshot_failed(&ScreenshotError::IncompatibleOwner);
                    }
                }
                ScreenshotResult::Conflict(None) => {
                    pending.screenshot = pending.screenshot.saturating_sub(1);
                    app.screenshot_failed(&ScreenshotError::Watcher);
                }
                ScreenshotResult::Stopped(candidates) => {
                    pending.screenshot = pending.screenshot.saturating_sub(1);
                    app.queue_screenshot_candidates(candidates);
                    app.screenshot_stopped();
                    capture.release_when_drained = true;
                }
                ScreenshotResult::Failed { error, retain_lock } => {
                    pending.screenshot = pending.screenshot.saturating_sub(1);
                    if !retain_lock {
                        capture.lease = None;
                    }
                    capture.release_when_drained = false;
                    app.screenshot_failed(&error);
                }
            }
            Ok(true)
        },
    )
}
