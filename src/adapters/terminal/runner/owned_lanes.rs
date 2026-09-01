//! Runtime-owned worker lanes and their shared shutdown boundary.

use crate::adapters::{control::ControlServer, process::CancellationFlag};

use super::{AccessibilityLane, ExternalLane, InputLane, PersistenceLane, TerminalError};
use crate::adapters::terminal::runner::finish::CleanupStage;
use crate::adapters::terminal::screenshot_lane::ScreenshotLane;

pub(super) struct OwnedLanes {
    pub(super) accessibility: AccessibilityLane,
    pub(super) control: Option<ControlServer>,
    pub(super) input: InputLane,
    pub(super) persistence: PersistenceLane,
    pub(super) external: ExternalLane,
    pub(super) update: crate::adapters::terminal::update_lane::UpdateLane,
    pub(super) screenshot: ScreenshotLane,
    pub(super) cancellation: CancellationFlag,
}

impl OwnedLanes {
    pub(super) fn request_stop(&mut self) {
        self.cancellation.cancel();
        self.accessibility.request_stop();
        self.input.request_stop();
        if let Some(control) = self.control.as_ref() {
            control.request_stop();
        }
        self.persistence.request_stop();
        self.external.request_stop();
        self.update.request_stop();
        self.screenshot.request_stop();
    }

    pub(super) fn stop_control(
        &mut self,
        deadline: crate::adapters::terminal::supervisor::ShutdownDeadline,
    ) -> Result<(), TerminalError> {
        self.control.take().map_or(Ok(()), |server| {
            server.stop_before(deadline.instant()).map_err(Into::into)
        })
    }

    pub(super) fn stop_workers(
        mut self,
        deadline: crate::adapters::terminal::supervisor::ShutdownDeadline,
    ) -> [(CleanupStage, Result<(), TerminalError>); 6] {
        self.request_stop();
        [
            (
                CleanupStage::Accessibility,
                self.accessibility.stop(deadline),
            ),
            (CleanupStage::Input, self.input.stop(deadline)),
            (CleanupStage::Persistence, self.persistence.stop(deadline)),
            (CleanupStage::External, self.external.stop(deadline)),
            (CleanupStage::Update, self.update.stop(deadline)),
            (CleanupStage::Screenshot, self.screenshot.stop(deadline)),
        ]
    }
}
