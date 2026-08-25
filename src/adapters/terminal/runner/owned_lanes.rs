//! Runtime-owned worker lanes and their shared shutdown boundary.

use crate::adapters::{control::ControlServer, process::CancellationFlag};

use super::{ExternalLane, InputLane, PersistenceLane, TerminalError};

pub(super) struct OwnedLanes {
    pub(super) control: Option<ControlServer>,
    pub(super) input: InputLane,
    pub(super) persistence: PersistenceLane,
    pub(super) external: ExternalLane,
    pub(super) update: crate::adapters::terminal::update_lane::UpdateLane,
    pub(super) cancellation: CancellationFlag,
}

impl OwnedLanes {
    pub(super) fn request_stop(&mut self) {
        self.cancellation.cancel();
        self.input.request_stop();
        if let Some(control) = self.control.as_ref() {
            control.request_stop();
        }
        self.persistence.request_stop();
        self.external.request_stop();
        self.update.request_stop();
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
    ) -> [Result<(), TerminalError>; 4] {
        self.request_stop();
        [
            self.input.stop(deadline),
            self.persistence.stop(deadline),
            self.external.stop(deadline),
            self.update.stop(deadline),
        ]
    }
}
