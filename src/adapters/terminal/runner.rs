//! Bounded UI and persistence lane composition.

mod accessibility_results;
mod admission;
mod composition;
mod diagnostics;
mod durability;
mod external_results;
mod fairness;
pub(super) mod finish;
mod heartbeat;
mod input_admission;
mod owned_lanes;
mod owner_control;
mod pending;
mod release_highlights;
mod restart;
mod screenshot_results;
mod termination;
mod update_results;

use std::{
    io::{IsTerminal, Stdout, stdin, stdout},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    adapters::{
        control::ControlServer,
        editor::RopeEditorFactory,
        runtime::{
            FileCaptureLease, FileRuntimeCoordinator, FileSchemaLease, FileSessionLease,
            SystemClock, SystemIdGenerator, SystemMonotonicClock,
        },
        sqlite::SqliteStore,
    },
    application::AppState,
    domain::SessionId,
    ports::{
        environment::{Clock as _, MonotonicClock},
        runtime::InstanceInfo,
        store::Store as _,
    },
    ui::{BoardApp, Theme, UiInput, UiKey, render_with_outcome},
};

use super::{
    TerminalError,
    accessibility_lane::AccessibilityLane,
    control::{PanicHookGuard, TerminationGuard},
    external::ExternalLane,
    input::{InputLane, InputMessage},
    persistence::PersistenceLane,
    screenshot_lane::ScreenshotLane,
};

use durability::{drain_persistence, enqueue_effects, storage_error_code};
use finish::CleanupStage::{Control, TerminalRestoration};
use heartbeat::PaneHeartbeat;
use owned_lanes::OwnedLanes;
use pending::{PendingControl, PendingWork};
use termination::{TerminationAdmission, admit_requested};

pub(crate) struct TerminalResources {
    pub(crate) state: AppState,
    pub(crate) store: SqliteStore,
    pub(crate) coordinator: FileRuntimeCoordinator,
    pub(crate) clock: SystemClock,
    pub(crate) ids: SystemIdGenerator,
    pub(crate) cwd: PathBuf,
    pub(crate) session_lease: FileSessionLease,
    pub(crate) schema_lease: FileSchemaLease,
    pub(crate) settings: super::LoadedSettings,
    pub(crate) recovery_directory: PathBuf,
    pub(crate) attachment_directory: PathBuf,
    pub(crate) installation: Option<crate::domain::Installation>,
    pub(crate) cache_directory: PathBuf,
    pub(crate) state_root: Option<PathBuf>,
    pub(crate) executable: PathBuf,
}

pub(crate) fn require_interactive() -> Result<(), TerminalError> {
    if stdin().is_terminal() && stdout().is_terminal() {
        Ok(())
    } else {
        Err(TerminalError::Io(
            "interactive launch requires a terminal; use --json for scriptable output".to_owned(),
        ))
    }
}

pub(super) struct WorkerLanes<'a> {
    pub(super) accessibility: &'a AccessibilityLane,
    pub(super) input: &'a InputLane,
    pub(super) persistence: &'a PersistenceLane,
    pub(super) external: &'a ExternalLane,
    pub(super) control: Option<&'a ControlServer>,
    pub(super) update: &'a super::update_lane::UpdateLane,
    pub(super) screenshot: &'a ScreenshotLane,
    pub(super) notification: &'a super::notification::PauseNotificationRouter,
    pub(super) monotonic: &'a dyn MonotonicClock,
    pub(super) termination: &'a TerminationGuard,
    pub(super) instance: &'a InstanceInfo,
    pub(super) cancellation: &'a crate::adapters::process::CancellationFlag,
}

#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "orthogonal watcher, requester, takeover, and release facts have independent transitions"
)]
pub(super) struct CaptureRuntime {
    lease: Option<FileCaptureLease>,
    release_when_drained: bool,
    shutdown_requested: bool,
    takeover_delivery: Option<crate::adapters::control::ControlDeliveryReceipt>,
    takeover_stopping: bool,
    watcher_stopped: bool,
    release_deadline: Option<Instant>,
}

/// Returns a typed setup, render, input, persistence, worker, or restoration failure.
#[expect(
    clippy::too_many_lines,
    reason = "runtime composition keeps ownership and cleanup order visible"
)]
pub(crate) fn run(resources: TerminalResources) -> Result<SessionId, TerminalError> {
    require_interactive()?;
    let TerminalResources {
        state,
        mut store,
        coordinator,
        clock,
        mut ids,
        cwd,
        mut session_lease,
        schema_lease,
        settings,
        recovery_directory,
        attachment_directory,
        installation,
        cache_directory,
        state_root,
        executable,
    } = resources;
    let session_id = state.board.session.id;
    store.recover_submissions(session_id, clock.now())?;
    let release_highlight_selection =
        release_highlights::load(&cache_directory, installation.as_ref(), session_id);
    let (control, control_warning) = composition::start_optional_control(&mut session_lease);
    let (theme, guard) =
        composition::enter_terminal(&settings.theme, settings.ui.keyboard_enhancement)?;
    let panic_hook = PanicHookGuard::install();
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let presentation_source = format!("proqi-{}", session_lease.info().instance_id);
    let invocation_roots = settings.invocation_roots.clone();
    let screenshot_settings = settings.screenshot.clone();
    let screenshot_activity = settings.screenshot.activity_policy();
    let terminal_host = super::host::TerminalHost::detect();
    let terminal_host_label = terminal_host.label();
    let notification = super::notification::PauseNotificationRouter::new(
        settings.screenshot.notify_terminal_on_auto_pause(),
        crate::adapters::herdr::HerdrEnvironment::detect(),
        &terminal_host,
    );
    let mut owned = composition::spawn_lanes(
        control,
        store,
        coordinator,
        cwd.clone(),
        recovery_directory,
        attachment_directory,
        presentation_source,
        cache_directory,
        installation.clone(),
        session_lease.info().instance_id,
        invocation_roots,
        screenshot_settings,
        session_lease.info().clone(),
        terminal_host_label,
        executable,
    );
    let mut pane_heartbeat = None;
    let shutdown = super::supervisor::ShutdownCoordinator::default();
    let check_for_updates = settings.ui.check_for_updates;
    let mut app =
        BoardApp::with_settings_and_cwd(state, settings.ui, cwd.clone(), RopeEditorFactory);
    app.install_release_highlights(
        release_highlight_selection.installed,
        release_highlight_selection.automatic,
        owned.input.latest_sequence(),
    );
    app.configure_screenshot_activity(screenshot_activity);
    if let Some(warning) = control_warning {
        app.set_warning(warning);
    }
    let monotonic = SystemMonotonicClock::default();
    let lanes = WorkerLanes {
        accessibility: &owned.accessibility,
        input: &owned.input,
        persistence: &owned.persistence,
        external: &owned.external,
        control: owned.control.as_ref(),
        update: &owned.update,
        screenshot: &owned.screenshot,
        notification: &notification,
        monotonic: &monotonic,
        termination: &termination,
        instance: session_lease.info(),
        cancellation: &owned.cancellation,
    };
    let run_result = owned.update.check(check_for_updates).and_then(|()| {
        drive(
            &mut terminal,
            &mut app,
            &lanes,
            &mut ids,
            clock,
            theme,
            &mut pane_heartbeat,
            &shutdown,
        )
    });
    let requested_restart = app.update_restart().cloned();
    if let Some(heartbeat) = pane_heartbeat.as_mut() {
        let _cleared = heartbeat.clear(&owned.external);
    }
    diagnostics::begin_shutdown(&mut owned);
    let shutdown_deadline = shutdown.request();
    drop(terminal);
    let restoration_result = guard.finish();
    drop(panic_hook);
    let control_result = owned.stop_control(shutdown_deadline);
    drop((session_lease, schema_lease));
    let lane_results = owned.stop_workers(shutdown_deadline);
    finish::runtime(
        run_result,
        lane_results.into_iter().chain([
            (Control, control_result),
            (TerminalRestoration, restoration_result),
        ]),
        shutdown_deadline.elapsed(),
    )?;
    restart::resume_after_update(
        installation.as_ref(),
        requested_restart.as_ref(),
        session_id,
        state_root.as_deref(),
    )
}

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the terminal event loop keeps its injected runtime boundaries explicit"
)]
fn drive(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    theme: Theme,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
    shutdown: &super::supervisor::ShutdownCoordinator,
) -> Result<(), TerminalError> {
    let mut pending = PendingWork::default();
    let mut capture = CaptureRuntime::default();
    let mut edit_generation = app.edit_generation();
    let mut edit_deadline = None;
    let mut refresh_deadlines = input_admission::RefreshDeadlines::default();
    let mut termination = TerminationAdmission::default();
    let mut held_input = None;
    enqueue_effects(app, lanes, BoardApp::discover_agents(), &mut pending)?;
    accessibility_results::start(app, lanes, &mut pending)?;
    let invocation_effects = app.refresh_invocations();
    enqueue_effects(app, lanes, invocation_effects, &mut pending)?;
    let mut redraw = true;
    loop {
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
        let (workers_changed, worker_backlog) = drain_workers(
            app,
            lanes,
            &mut pending,
            &mut capture,
            ids,
            clock,
            pane_heartbeat,
        )?;
        redraw |= workers_changed || app.expire_update_barrier(clock.now());
        if app.quit && app.screenshot_retry_ready() && !termination.is_admitted() {
            app.retain_failed_capture_after_quit();
            redraw = true;
        }
        if termination.is_admitted() && app.screenshot_retry_ready() {
            let mut effects = app.handle(UiInput::Key(UiKey::Quit), ids, &clock);
            if !app.quit && app.screenshot_retry_ready() {
                effects.extend(app.handle(UiInput::Key(UiKey::Quit), ids, &clock));
            }
            enqueue_effects(app, lanes, effects, &mut pending)?;
            redraw = true;
        }
        if termination.is_admitted() && !app.screenshot_commit_pending() {
            let effects = app.flush_pending_edit(ids, &clock);
            if !effects.is_empty() {
                enqueue_effects(app, lanes, effects, &mut pending)?;
                edit_deadline = None;
                redraw = true;
            }
        }
        let release_effects = screenshot_results::release_if_drained(app, &mut capture);
        if !release_effects.is_empty() {
            enqueue_effects(app, lanes, release_effects, &mut pending)?;
            redraw = true;
        }
        let capture_effects = if admission::capture(app, &pending).is_ok()
            && (!app.quit || capture.shutdown_requested)
        {
            app.advance_screenshot_capture(ids, &clock)
        } else {
            Vec::new()
        };
        if !capture_effects.is_empty() {
            enqueue_effects(app, lanes, capture_effects, &mut pending)?;
            redraw = true;
        }
        if app.edit_generation() != edit_generation {
            edit_generation = app.edit_generation();
            edit_deadline = app
                .has_pending_edit()
                .then(|| Instant::now() + Duration::from_millis(250));
        }
        if edit_deadline.is_some_and(|deadline| Instant::now() >= deadline)
            && !app.screenshot_commit_pending()
        {
            let effects = app.flush_pending_edit(ids, &clock);
            enqueue_effects(app, lanes, effects, &mut pending)?;
            edit_deadline = None;
            redraw = true;
        }
        input_admission::refresh_if_due(app, lanes, &mut pending, &mut refresh_deadlines)?;
        if let Some(heartbeat) = pane_heartbeat.as_mut() {
            let _refreshed = heartbeat.refresh_if_due(lanes.external);
        }
        if redraw {
            let mut release_highlights_visible = false;
            terminal.draw(|frame| {
                let layout = app.prepare_frame(frame.area());
                release_highlights_visible = render_with_outcome(frame, app, &layout, &theme);
            })?;
            app.arm_update_prompt();
            if release_highlights_visible {
                app.arm_release_highlights(lanes.input.latest_sequence());
            }
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
            let deadline = shutdown.request();
            if !capture.shutdown_requested && capture.lease.is_some() {
                lanes.screenshot.shutdown(deadline)?;
                pending.screenshot = pending.screenshot.saturating_add(1);
                capture.shutdown_requested = true;
                capture.release_deadline = Some(deadline.instant());
            }
            if app.update_restart().is_none() {
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
                return termination.outcome(&app.state.durability);
            }
            if deadline.expired() {
                return Err(TerminalError::Worker(
                    "runtime shutdown exceeded its shared deadline",
                ));
            }
            thread::sleep(Duration::from_millis(5));
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

fn drain_workers(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
) -> Result<(bool, bool), TerminalError> {
    let persistence = drain_persistence(app, lanes, pending, ids, &clock)?;
    let accessibility = accessibility_results::drain(app, lanes, pending)?;
    let external = external_results::drain(app, lanes, pending, ids, clock, pane_heartbeat)?;
    let control = owner_control::drain(app, lanes, pending, capture, ids, clock)?;
    let update = update_results::drain(app, lanes, pending)?;
    let screenshot =
        screenshot_results::drain(app, lanes, pending, capture, lanes.monotonic.now())?;
    let changed = persistence.changed
        || accessibility.changed
        || external.changed
        || control.changed
        || update.changed
        || screenshot.changed;
    let backlog = persistence.budget_exhausted
        || accessibility.budget_exhausted
        || external.budget_exhausted
        || control.budget_exhausted
        || update.budget_exhausted
        || screenshot.budget_exhausted;
    Ok((changed, backlog))
}

pub(super) fn supports_true_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::env::var("COLORTERM")
        .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "truecolor" | "24bit"))
        || std::env::var("TERM").is_ok_and(|value| value.to_ascii_lowercase().contains("direct"))
}
