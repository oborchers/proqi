//! Monotonic admission for supported process-termination signals.

use crate::{
    adapters::{
        runtime::{SystemClock, SystemIdGenerator},
        terminal::{TerminalError, supervisor::ShutdownCoordinator},
    },
    application::DurabilityState,
    ui::{BoardApp, UiInput, UiKey},
};

use super::{PendingWork, WorkerLanes, durability::enqueue_effects};

#[derive(Default)]
pub(super) struct TerminationAdmission {
    admitted: bool,
}

impl TerminationAdmission {
    pub(super) fn admit(&mut self) -> bool {
        if self.admitted {
            return false;
        }
        self.admitted = true;
        true
    }

    pub(super) const fn is_admitted(&self) -> bool {
        self.admitted
    }

    pub(super) const fn shutdown_requested(&self, ui_quit: bool) -> bool {
        self.admitted || ui_quit
    }

    pub(super) fn outcome(&self, durability: &DurabilityState) -> Result<(), TerminalError> {
        if self.admitted && matches!(durability, DurabilityState::Failed { .. }) {
            return Err(TerminalError::Worker(
                "termination completed with non-durable work",
            ));
        }
        Ok(())
    }
}

pub(super) fn admit_requested(
    admission: &mut TerminationAdmission,
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    shutdown: &ShutdownCoordinator,
    pending: &mut PendingWork,
) -> Result<(), TerminalError> {
    if !lanes.termination.requested() || !admission.admit() {
        return Ok(());
    }
    let _deadline = shutdown.request();
    lanes.cancellation.cancel();
    if let Some(control) = lanes.control {
        control.request_stop();
    }
    let mut effects = app.handle(UiInput::Key(UiKey::Quit), ids, &clock);
    if !app.quit && app.screenshot_retry_ready() {
        effects.extend(app.handle(UiInput::Key(UiKey::Quit), ids, &clock));
    }
    enqueue_effects(app, lanes, effects, pending)
}

#[cfg(test)]
mod tests {
    use crate::{
        application::{DurabilityState, FailureCode},
        domain::OperationSequence,
    };

    use super::TerminationAdmission;

    #[test]
    fn termination_admission_is_monotonic_when_ui_quit_is_revoked() {
        let mut admission = TerminationAdmission::default();
        assert!(!admission.shutdown_requested(false));
        assert!(admission.admit());
        assert!(!admission.admit());
        assert!(admission.shutdown_requested(false));
        assert!(
            admission
                .outcome(&DurabilityState::Failed {
                    durable: OperationSequence::default(),
                    failed: OperationSequence::new(1),
                    code: FailureCode::StorageFailed,
                })
                .is_err()
        );
    }
}
