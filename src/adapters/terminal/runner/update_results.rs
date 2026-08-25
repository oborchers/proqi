//! Bounded application of background update results.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::terminal::{
        TerminalError,
        update_lane::{UpdateActionResult, UpdateResult},
    },
    application::{UpdateExecution, UpdateExecutionStatus},
    ui::BoardApp,
};

use super::{
    PendingWork, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
};

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
) -> Result<DrainOutcome, TerminalError> {
    let disconnected_is_clean = pending.update == 0;
    drain_bounded(
        || match lanes.update.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if disconnected_is_clean => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(TerminalError::Worker("update result lane disconnected"))
            }
        },
        |result| {
            apply_result(app, pending, result);
            Ok(true)
        },
    )
}

fn apply_result(app: &mut BoardApp, pending: &mut PendingWork, result: UpdateResult) {
    match result {
        UpdateResult::Notice(notice) => {
            app.present_update(notice.version, notice.installation, notice.participants);
        }
        UpdateResult::Action(result) => {
            pending.update = pending.update.saturating_sub(1);
            app.complete_update_action(action_message(result));
        }
    }
}

fn action_message(
    result: Result<UpdateActionResult, crate::ports::update::UpdateError>,
) -> Result<String, String> {
    match result {
        Ok(UpdateActionResult::Dismissed) => Ok("Update deferred for now.".to_owned()),
        Ok(UpdateActionResult::Skipped) => Ok("This release will no longer be offered.".to_owned()),
        Ok(UpdateActionResult::Instructions(version)) => Ok(format!(
            "Proqi {version} is available at https://github.com/oborchers/proqi/releases/latest"
        )),
        Ok(UpdateActionResult::Executed(execution)) => execution_message(&execution),
        Err(_) => Err("Update failed. Every session remains saved and usable.".to_owned()),
    }
}

fn execution_message(execution: &UpdateExecution) -> Result<String, String> {
    match &execution.status {
        UpdateExecutionStatus::Installed { version } if execution.restart_failed.is_empty() => {
            Ok(format!(
                "Proqi {version} installed. Restarting {} session(s).",
                execution.restart_requests
            ))
        }
        UpdateExecutionStatus::Installed { version } => Err(format!(
            "Proqi {version} installed, but {} session(s) could not restart. Resume them normally.",
            execution.restart_failed.len()
        )),
        UpdateExecutionStatus::AlreadyInProgress => {
            Ok("Another Proqi session is already updating this installation.".to_owned())
        }
        UpdateExecutionStatus::Aborted { code, .. } => Err(format!(
            "Update stopped safely before installation ({code}). Every session remains usable."
        )),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator,
        application::{UpdateExecution, UpdateExecutionStatus},
        domain::StableVersion,
        ports::environment::IdGenerator as _,
    };

    use super::execution_message;

    #[test]
    fn partial_restart_reports_a_safe_recovery_message() {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let execution = UpdateExecution {
            operation_id: ids.request_id(),
            prepared_participants: 2,
            restart_requests: 2,
            restart_failed: vec![ids.instance_id()],
            convergence_state_recorded: true,
            status: UpdateExecutionStatus::Installed {
                version: StableVersion::parse("1.2.3").expect("valid version"),
            },
        };

        let message = execution_message(&execution).expect_err("partial restart must warn");
        assert!(message.contains("1 session(s) could not restart"));
        assert!(message.contains("Resume them normally"));
    }
}
