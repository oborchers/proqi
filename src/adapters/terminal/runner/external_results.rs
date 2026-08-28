//! Bounded application of external worker results.

use std::{io::Write as _, sync::mpsc::TryRecvError};

use crate::{
    adapters::terminal::{TerminalError, external::ExternalResult},
    application::{ClipboardIntent, FailureCode},
    ports::clipboard::ClipboardWrite,
    ui::BoardApp,
};

use super::{
    PendingWork, SystemClock, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
    heartbeat::PaneHeartbeat,
};

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
) -> Result<DrainOutcome, TerminalError> {
    drain_bounded(
        || match lanes.external.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if lanes.external.stopped_cleanly() => Ok(None),
            Err(TryRecvError::Disconnected) => Err(lanes
                .external
                .worker_failure()
                .unwrap_or(TerminalError::Worker("external result lane disconnected"))),
        },
        |result| {
            pending.external = pending.external.saturating_sub(1);
            let effects = complete(result, app, ids, clock, pane_heartbeat, lanes.external);
            super::durability::enqueue_effects(app, lanes, effects, pending)?;
            Ok(true)
        },
    )
}

fn complete(
    result: ExternalResult,
    app: &mut BoardApp,
    ids: &mut crate::adapters::runtime::SystemIdGenerator,
    clock: SystemClock,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
    external: &crate::adapters::terminal::external::ExternalLane,
) -> Vec<crate::application::Effect> {
    match result {
        ExternalResult::Written {
            request_id,
            intent,
            result,
        } => {
            let succeeded = match result {
                Ok(ClipboardWrite::Native) => true,
                Ok(ClipboardWrite::Osc52(sequence)) => {
                    let emitted = write_osc52(&sequence).is_ok();
                    emitted && intent == ClipboardIntent::Copy
                }
                Err(_) => false,
            };
            app.complete_clipboard_write(
                request_id,
                succeeded.then_some(()).ok_or(FailureCode::ClipboardFailed),
                ids,
                &clock,
            )
        }
        ExternalResult::Read { request_id, result } => app.complete_clipboard_read_payload(
            request_id,
            result.map_err(|_| FailureCode::ClipboardFailed),
            ids,
            &clock,
        ),
        ExternalResult::Exported { request_id, result } => {
            app.complete_recovery_export(request_id, result.map_err(|error| error.to_string()))
        }
        ExternalResult::AgentsDiscovered { pane_id, result } => {
            publish_discovered_identity(pane_heartbeat, pane_id, external);
            app.complete_agent_discovery(result);
            Vec::new()
        }
        ExternalResult::InvocationsDiscovered(result) => {
            app.complete_invocation_discovery(result);
            Vec::new()
        }
        ExternalResult::AgentSubmitted {
            submission_id,
            result,
        } => {
            let outcome = result
                .as_ref()
                .as_ref()
                .map_or_else(|error| agent_error_code(error), |_| "accepted");
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::SubmissionState {
                    submission_id,
                    state: "delivered",
                    outcome: Some(outcome),
                },
            );
            app.complete_submission(submission_id, *result)
        }
    }
}

const fn agent_error_code(error: &crate::ports::agent::AgentError) -> &'static str {
    error.stable_code().as_str()
}

fn publish_discovered_identity(
    heartbeat: &mut Option<PaneHeartbeat>,
    pane_id: Option<String>,
    external: &crate::adapters::terminal::external::ExternalLane,
) {
    if heartbeat.is_some() {
        return;
    }
    let Some(mut discovered) = pane_id.and_then(PaneHeartbeat::from_pane_id) else {
        return;
    };
    let _published = discovered.publish(external);
    *heartbeat = Some(discovered);
}

fn write_osc52(sequence: &[u8]) -> std::io::Result<()> {
    let output = std::io::stdout();
    let mut writer = output.lock();
    writer.write_all(sequence)?;
    writer.flush()
}
