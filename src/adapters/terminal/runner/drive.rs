//! Bounded terminal event loop and exact input-recovery admission.

use std::{
    io::Stdout,
    thread,
    time::{Duration, Instant},
};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    adapters::{
        control::ControlServer,
        runtime::{
            FileSchemaLease, SystemClock, SystemIdGenerator,
            input_recovery::{InputRecovery, RecoveryFailure, StallDecision},
        },
    },
    domain::SessionId,
    ports::environment::{Clock as _, IdGenerator as _},
    ui::{BoardApp, Theme, render_with_outcome},
};

use super::super::{
    TerminalError,
    input::{InputFailure, InputMessage},
};
use super::termination::admit_requested;
use super::{
    CaptureRuntime, PaneHeartbeat, PendingWork, TerminationAdmission, WorkerLanes,
    accessibility_results, admission, continuity, enqueue_effects, input_admission,
    screenshot_results, worker_results,
};

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the terminal event loop keeps its injected runtime boundaries explicit"
)]
pub(super) fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    theme: Theme,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
    shutdown: &super::super::supervisor::ShutdownCoordinator,
    schema_lease: &mut Option<FileSchemaLease>,
    input_recovery: &mut InputRecovery,
    recovery_disposition: &mut continuity::RecoveryDisposition,
    executable: &std::path::Path,
    state_root: Option<&std::path::Path>,
    session_id: SessionId,
    pending_transfers: Vec<crate::adapters::sqlite::PendingTransfer>,
) -> Result<(), TerminalError> {
    let mut pending = PendingWork::default();
    let mut capture = CaptureRuntime::default();
    let mut edit_generation = app.edit_generation();
    let mut edit_deadline = None;
    let mut refresh_deadlines = input_admission::RefreshDeadlines::default();
    let mut termination = TerminationAdmission::default();
    let mut recovery_export_attempted = false;
    let mut held_input = None;
    enqueue_effects(app, lanes, BoardApp::discover_agents(), &mut pending)?;
    let recovery = app.recover_pending_transfers(
        pending_transfers
            .into_iter()
            .map(|pending| pending.request)
            .collect(),
    );
    enqueue_effects(app, lanes, recovery, &mut pending)?;
    accessibility_results::start(app, lanes, &mut pending)?;
    let invocation_effects = app.refresh_invocations();
    enqueue_effects(app, lanes, invocation_effects, &mut pending)?;
    let mut redraw = true;
    loop {
        note_probation_progress(input_recovery, lanes);
        admit_requested(
            &mut termination,
            app,
            lanes,
            ids,
            clock,
            shutdown,
            &mut pending,
        )?;
        if !termination.is_admitted() {
            let effects = app.advance_screenshot_activity(lanes.monotonic.now());
            enqueue_effects(app, lanes, effects, &mut pending)?;
        }
        let mut drain_context =
            worker_results::DrainContext::new(ids, clock, pane_heartbeat, schema_lease);
        let (workers_changed, worker_backlog) =
            worker_results::drain(app, lanes, &mut pending, &mut capture, &mut drain_context)?;
        redraw |= workers_changed || app.expire_update_barrier(clock.now());
        settle_pending_state(
            app,
            lanes,
            ids,
            clock,
            &mut pending,
            &mut capture,
            &termination,
            recovery_disposition.is_recovery_shutdown(),
            &mut recovery_export_attempted,
            &mut edit_generation,
            &mut edit_deadline,
            &mut redraw,
        )?;
        input_admission::refresh_if_due(app, lanes, &mut pending, &mut refresh_deadlines)?;
        if let Some(heartbeat) = pane_heartbeat.as_mut() {
            let _refreshed = heartbeat.refresh_if_due(lanes.external);
        }
        if redraw {
            draw(terminal, app, lanes, theme)?;
            redraw = false;
        }
        if let Some((sequence, event)) = held_input.take() {
            if app.screenshot_barrier_accepts(&event) {
                input_admission::apply(
                    app,
                    lanes,
                    ids,
                    clock,
                    &mut pending,
                    &mut refresh_deadlines,
                    sequence,
                    event,
                )?;
                redraw = true;
            } else {
                held_input = Some((sequence, event));
            }
        }
        if termination.shutdown_requested(app.quit) {
            if shutdown_ready(
                app,
                lanes,
                &mut pending,
                &mut capture,
                shutdown,
                recovery_disposition.is_recovery_shutdown(),
            )? {
                checkpoint_recovery(
                    app,
                    input_recovery,
                    *recovery_disposition == continuity::RecoveryDisposition::Replace,
                    executable,
                    state_root,
                    session_id,
                )?;
                return termination.outcome(&app.state.durability);
            }
            continue;
        }
        if held_input.is_some() {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        let input_wait = if worker_backlog {
            Duration::ZERO
        } else {
            Duration::from_millis(30)
        };
        match lanes.input.receiver.recv_timeout(input_wait) {
            Ok(InputMessage::Event {
                sequence,
                input: event,
            }) => {
                if !app.screenshot_barrier_accepts(&event) {
                    held_input = Some((sequence, event));
                    continue;
                }
                input_admission::apply(
                    app,
                    lanes,
                    ids,
                    clock,
                    &mut pending,
                    &mut refresh_deadlines,
                    sequence,
                    event,
                )?;
                redraw = true;
            }
            Ok(InputMessage::Failed(InputFailure::Unresponsive)) => {
                admit_recovery(
                    app,
                    lanes,
                    ids,
                    clock,
                    shutdown,
                    input_recovery,
                    recovery_disposition,
                    &mut termination,
                    &mut pending,
                    session_id,
                )?;
                redraw = true;
            }
            Ok(InputMessage::Failed(failure)) => {
                return Err(TerminalError::Io(failure.to_string()));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(TerminalError::Worker("input lane disconnected"));
            }
        }
    }
}

fn checkpoint_recovery(
    app: &BoardApp,
    recovery: &mut InputRecovery,
    recovering: bool,
    executable: &std::path::Path,
    state_root: Option<&std::path::Path>,
    session_id: SessionId,
) -> Result<(), TerminalError> {
    if !recovering {
        return Ok(());
    }
    let Some(state) = app.input_recovery_state() else {
        return fail_closed(
            RecoveryFailure::StateUnsupported,
            recovery,
            executable,
            state_root,
            session_id,
        );
    };
    recovery.checkpoint(state).map_err(|error| {
        continuity::record(
            "recovery_prepared",
            Some(error.failure()),
            recovery.attempt_count(),
            Some("failed_closed"),
        );
        continuity::stalled(error.failure(), executable, state_root, session_id)
    })
}

fn note_probation_progress(recovery: &mut InputRecovery, lanes: &WorkerLanes<'_>) {
    match recovery.prove_healthy(lanes.input.completed_polls()) {
        Ok(true) => continuity::record("healthy", None, recovery.attempt_count(), Some("proved")),
        Ok(false) => {}
        Err(error) => continuity::record(
            "healthy",
            Some(error.failure()),
            recovery.attempt_count(),
            Some("disabled"),
        ),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "ordered UI settlement uses explicit owners"
)]
fn settle_pending_state(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    termination: &TerminationAdmission,
    recovery_shutdown: bool,
    recovery_export_attempted: &mut bool,
    edit_generation: &mut u64,
    edit_deadline: &mut Option<Instant>,
    redraw: &mut bool,
) -> Result<(), TerminalError> {
    if recovery_shutdown
        && !*recovery_export_attempted
        && let Some(effects) = app.export_failed_state_for_shutdown(ids, &clock)
    {
        *recovery_export_attempted = true;
        enqueue_effects(app, lanes, effects, pending)?;
        *redraw = true;
    }
    if app.quit && app.screenshot_retry_ready() && !termination.is_admitted() {
        app.retain_failed_capture_after_quit();
        *redraw = true;
    }
    if termination.is_admitted() && app.screenshot_retry_ready() {
        let effects = app.handle_termination_request(ids, &clock);
        enqueue_effects(app, lanes, effects, pending)?;
        *redraw = true;
    }
    if termination.is_admitted() && !app.screenshot_commit_pending() {
        let effects = app.flush_pending_edit(ids, &clock);
        if !effects.is_empty() {
            enqueue_effects(app, lanes, effects, pending)?;
            *edit_deadline = None;
            *redraw = true;
        }
    }
    let release_effects = screenshot_results::release_if_drained(app, capture);
    if !release_effects.is_empty() {
        enqueue_effects(app, lanes, release_effects, pending)?;
        *redraw = true;
    }
    let capture_effects =
        if admission::capture(app, pending).is_ok() && (!app.quit || capture.shutdown_requested) {
            app.advance_screenshot_capture(ids, &clock)
        } else {
            Vec::new()
        };
    if !capture_effects.is_empty() {
        enqueue_effects(app, lanes, capture_effects, pending)?;
        *redraw = true;
    }
    if app.edit_generation() != *edit_generation {
        *edit_generation = app.edit_generation();
        *edit_deadline = app
            .has_pending_edit()
            .then(|| Instant::now() + Duration::from_millis(250));
    }
    if edit_deadline.is_some_and(|deadline| Instant::now() >= deadline)
        && !app.screenshot_commit_pending()
    {
        let effects = app.flush_pending_edit(ids, &clock);
        enqueue_effects(app, lanes, effects, pending)?;
        *edit_deadline = None;
        *redraw = true;
    }
    Ok(())
}

fn draw(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    theme: Theme,
) -> Result<(), TerminalError> {
    let mut release_highlights_visible = false;
    terminal.draw(|frame| {
        let layout = app.prepare_frame(frame.area());
        release_highlights_visible = render_with_outcome(frame, app, &layout, &theme);
    })?;
    app.arm_update_prompt();
    let input_boundary = lanes.input.latest_sequence();
    app.note_release_highlights_rendered(release_highlights_visible, input_boundary);
    Ok(())
}

fn shutdown_ready(
    app: &BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    shutdown: &super::super::supervisor::ShutdownCoordinator,
    recovering: bool,
) -> Result<bool, TerminalError> {
    let deadline = shutdown.request();
    if !capture.shutdown_requested && capture.lease.is_some() {
        lanes.screenshot.shutdown(deadline)?;
        pending.screenshot = pending.screenshot.saturating_add(1);
        capture.shutdown_requested = true;
        capture.release_deadline = Some(deadline.instant());
    }
    if recovering || app.update_restart().is_none() {
        lanes.cancellation.cancel();
    }
    if let Some(control) = lanes.control {
        control.request_stop();
    }
    let control_quiescent = lanes.control.is_none_or(ControlServer::is_quiescent);
    let screenshot_quiescent = capture.lease.is_none()
        && (!capture.shutdown_requested || capture.watcher_stopped)
        && app.screenshot_shutdown_drained();
    if pending.is_empty() && control_quiescent && screenshot_quiescent {
        return Ok(true);
    }
    if deadline.expired() {
        return Err(TerminalError::Worker(
            "runtime shutdown exceeded its shared deadline",
        ));
    }
    thread::sleep(Duration::from_millis(5));
    Ok(false)
}

#[expect(
    clippy::too_many_arguments,
    reason = "recovery admission binds exact runtime state"
)]
fn admit_recovery(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    shutdown: &super::super::supervisor::ShutdownCoordinator,
    recovery: &mut InputRecovery,
    recovery_disposition: &mut continuity::RecoveryDisposition,
    termination: &mut TerminationAdmission,
    pending: &mut PendingWork,
    session_id: SessionId,
) -> Result<(), TerminalError> {
    match recovery.confirm_stall(clock.now(), ids.request_id(), session_id) {
        StallDecision::Recover { attempt_count } => {
            continuity::record("confirmed_stall", None, attempt_count, Some("admitted"));
            *recovery_disposition = continuity::RecoveryDisposition::Replace;
            begin_recovery_shutdown(app, lanes, ids, clock, shutdown, termination, pending)
        }
        StallDecision::FailClosed {
            reason,
            attempt_count,
        } => {
            continuity::record(
                "confirmed_stall",
                Some(reason),
                attempt_count,
                Some("failed_closed"),
            );
            *recovery_disposition = continuity::RecoveryDisposition::FailClosed(reason);
            begin_recovery_shutdown(app, lanes, ids, clock, shutdown, termination, pending)
        }
    }
}

fn begin_recovery_shutdown(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    shutdown: &super::super::supervisor::ShutdownCoordinator,
    termination: &mut TerminationAdmission,
    pending: &mut PendingWork,
) -> Result<(), TerminalError> {
    let _admitted = termination.admit();
    let _deadline = shutdown.request();
    lanes.cancellation.cancel();
    if let Some(control) = lanes.control {
        control.request_stop();
    }
    let effects = app.handle_termination_request(ids, &clock);
    enqueue_effects(app, lanes, effects, pending)
}

fn fail_closed(
    reason: RecoveryFailure,
    recovery: &InputRecovery,
    executable: &std::path::Path,
    state_root: Option<&std::path::Path>,
    session_id: SessionId,
) -> Result<(), TerminalError> {
    continuity::record(
        "confirmed_stall",
        Some(reason),
        recovery.attempt_count(),
        Some("failed_closed"),
    );
    Err(continuity::stalled(
        reason, executable, state_root, session_id,
    ))
}
