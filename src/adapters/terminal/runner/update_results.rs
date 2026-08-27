//! Bounded application of background update results.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::terminal::{
        TerminalError,
        update_lane::{ManualCheckResult, UpdateActionResult, UpdateResult},
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
    let input_boundary = lanes.input.latest_sequence();
    drain_bounded(
        || match lanes.update.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if lanes.update.stopped_cleanly() => Ok(None),
            Err(TryRecvError::Disconnected) => Err(lanes
                .update
                .worker_failure()
                .unwrap_or(TerminalError::Worker("update result lane disconnected"))),
        },
        |result| {
            apply_result(app, pending, result, input_boundary);
            Ok(true)
        },
    )
}

fn apply_result(
    app: &mut BoardApp,
    pending: &mut PendingWork,
    result: UpdateResult,
    input_boundary: u64,
) {
    match result {
        UpdateResult::Notice(notice) => {
            app.present_update_protected(
                notice.version,
                notice.installation,
                notice.participants,
                input_boundary,
            );
        }
        UpdateResult::Action(result) => {
            pending.update = pending.update.saturating_sub(1);
            app.complete_update_action(action_message(result));
        }
        UpdateResult::ManualCheck(result) => {
            pending.update = pending.update.saturating_sub(1);
            apply_manual_check(app, result, input_boundary);
        }
    }
}

fn apply_manual_check(
    app: &mut BoardApp,
    result: Result<ManualCheckResult, crate::ports::update::UpdateError>,
    input_boundary: u64,
) {
    match result {
        Ok(ManualCheckResult::Prompt(notice)) => app.present_update_protected(
            notice.version,
            notice.installation,
            notice.participants,
            input_boundary,
        ),
        Ok(ManualCheckResult::Current(version)) => {
            app.complete_update_action(Ok(format!("Proqi {version} is current.")));
        }
        Ok(ManualCheckResult::Suppressed(version)) => app.complete_update_action(Ok(format!(
            "Proqi {version} is available but skipped for this installation."
        ))),
        Ok(ManualCheckResult::InProgress) => app.complete_update_action(Ok(
            "Another Proqi session is checking for updates.".to_owned(),
        )),
        Ok(ManualCheckResult::Instructions(version)) => app.complete_update_action(Ok(format!(
            "Proqi {version} is available at https://github.com/oborchers/proqi/releases/latest"
        ))),
        Err(_) => app.complete_update_action(Err(
            "Update check failed. Proqi remains available offline.".to_owned(),
        )),
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
