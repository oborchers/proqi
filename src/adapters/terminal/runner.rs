//! Bounded UI and persistence lane composition.

mod accessibility_results;
mod admission;
mod capture_runtime;
pub(crate) mod composition;
mod continuity;
mod diagnostics;
mod drive;
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
mod worker_results;

use std::{io::stdout, path::PathBuf};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    adapters::{
        control::ControlServer,
        editor::RopeEditorFactory,
        runtime::{
            FileRuntimeCoordinator, FileSchemaLease, FileSessionLease, SystemClock,
            SystemIdGenerator, SystemMonotonicClock,
            input_recovery::{ExecutableIdentity, InputRecovery, RecoveryStage},
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
    ui::BoardApp,
};

use super::{
    TerminalError,
    accessibility_lane::AccessibilityLane,
    control::{PanicHookGuard, TerminationGuard},
    external::ExternalLane,
    input::InputLane,
    persistence::PersistenceLane,
    screenshot_lane::ScreenshotLane,
};

use capture_runtime::CaptureRuntime;
use durability::{enqueue_effects, storage_error_code};
use finish::CleanupStage::{Control, TerminalRestoration};
use heartbeat::PaneHeartbeat;
use owned_lanes::OwnedLanes;
use pending::{PendingControl, PendingWork};
use termination::TerminationAdmission;

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
    pub(crate) startup_admission: Option<Box<dyn crate::ports::update::UpdateLease>>,
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

/// Returns a typed setup, render, input, persistence, worker, or restoration failure.
#[expect(
    clippy::too_many_lines,
    reason = "runtime composition keeps ownership and cleanup order visible"
)]
pub(crate) fn run(resources: TerminalResources) -> Result<SessionId, TerminalError> {
    composition::require_interactive()?;
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
        startup_admission,
    } = resources;
    let mut schema_lease = Some(schema_lease);
    let session_id = state.board.session.id;
    let executable_identity = ExecutableIdentity::read(&executable);
    let runtime_directory = coordinator.runtime_directory().to_path_buf();
    let input_recovery = InputRecovery::open(
        coordinator.runtime_directory(),
        session_id,
        session_lease.info().instance_id,
        executable_identity.clone(),
        crate::adapters::process::input_recovery_startup_context(),
    );
    crate::adapters::diagnostics::record_recovery_admission(
        input_recovery
            .as_ref()
            .map(InputRecovery::admission)
            .map_err(|error| *error),
    );
    let mut input_recovery = input_recovery.map_err(|error| {
        continuity::unavailable(error, &executable, state_root.as_deref(), session_id)
    })?;
    if input_recovery.stage() == RecoveryStage::Probation {
        continuity::record(
            "probation",
            None,
            input_recovery.attempt_count(),
            Some("started"),
        );
    }
    store.recover_submissions(session_id, clock.now())?;
    let pending_transfers = store.pending_transfers(session_id)?;
    let release_highlight_selection =
        release_highlights::load(&cache_directory, installation.as_ref(), session_id);
    let (mut control, mut control_warning) = composition::start_optional_control(&session_lease);
    let (theme, guard) = composition::enter_terminal(
        &settings.theme,
        settings.ui.keyboard_enhancement,
        settings.ui.mouse_capture,
    )?;
    let panic_hook = PanicHookGuard::install(settings.ui.mouse_capture);
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
    let check_for_updates = settings.ui.check_for_updates;
    let mut app =
        BoardApp::with_settings_and_cwd(state, settings.ui, cwd.clone(), RopeEditorFactory);
    app.set_home_directory(crate::ports::environment::Environment::home_directory(
        &crate::adapters::runtime::SystemEnvironment,
    ));
    if let Some(recovery_state) = input_recovery.ui_state().cloned()
        && !app.restore_input_recovery_state(recovery_state)
    {
        return Err(continuity::stalled(
            crate::adapters::runtime::input_recovery::RecoveryFailure::StateUnsupported,
            &executable,
            state_root.as_deref(),
            session_id,
        ));
    }
    let control_ready = composition::publish_optional_control(
        &mut session_lease,
        &mut control,
        &mut control_warning,
    );
    let startup_admission =
        admission::retain_unpublished_startup_admission(startup_admission, control_ready);
    let mut owned = composition::spawn_lanes(
        control,
        store,
        coordinator,
        cwd.clone(),
        recovery_directory,
        attachment_directory,
        presentation_source,
        cache_directory.clone(),
        installation.clone(),
        session_lease.info().instance_id,
        invocation_roots,
        screenshot_settings,
        session_lease.info().clone(),
        terminal_host_label,
        executable.clone(),
        &runtime_directory,
    );
    let mut pane_heartbeat = None;
    let shutdown = super::supervisor::ShutdownCoordinator::default();
    let automatic_highlights_ready = release_highlights::mark_restart_ready(
        &cache_directory,
        installation.as_ref(),
        &release_highlight_selection,
        control_ready,
    );
    crate::adapters::diagnostics::record(crate::adapters::diagnostics::SafeEvent::RuntimeReady {
        control_ready,
    });
    app.install_release_highlights(
        release_highlight_selection.installed,
        release_highlight_selection
            .automatic
            .filter(|_| automatic_highlights_ready),
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
    let mut recovery_disposition = continuity::RecoveryDisposition::None;
    let run_result = owned.update.check(check_for_updates).and_then(|()| {
        drive::run(
            &mut terminal,
            &mut app,
            &lanes,
            &mut ids,
            clock,
            theme,
            &mut pane_heartbeat,
            &shutdown,
            &mut schema_lease,
            &mut input_recovery,
            &mut recovery_disposition,
            &executable,
            state_root.as_deref(),
            session_id,
            pending_transfers,
        )
    });
    let requested_restart = app
        .update_restart()
        .cloned()
        .zip(app.update_restart_operation());
    let previous_instance_id = session_lease.info().instance_id;
    let mut lifecycle_resources = Some((session_lease, schema_lease, startup_admission));
    if let Some(heartbeat) = pane_heartbeat.as_mut() {
        let _cleared = heartbeat.clear(&owned.external);
    }
    diagnostics::begin_shutdown(&mut owned);
    let shutdown_deadline = shutdown.request();
    drop(terminal);
    let restoration_result = guard.finish();
    drop(panic_hook);
    let control_result = owned.stop_control(shutdown_deadline);
    if !recovery_disposition.is_recovery_shutdown() {
        drop(lifecycle_resources.take());
    }
    let lane_results = owned.stop_workers(shutdown_deadline);
    let finish_result = finish::runtime(
        run_result,
        lane_results.into_iter().chain([
            (Control, control_result),
            (TerminalRestoration, restoration_result),
        ]),
        shutdown_deadline.elapsed(),
    );
    if let Err(error) = finish_result {
        if recovery_disposition.is_recovery_shutdown() {
            let reason = recovery_disposition.failure_reason();
            continuity::record(
                "recovery_prepared",
                Some(reason),
                input_recovery.attempt_count(),
                Some("failed_closed"),
            );
            return Err(continuity::preparation_failed(
                &error,
                reason,
                &executable,
                state_root.as_deref(),
                session_id,
                app.recovery_export_path(),
            ));
        }
        return Err(error);
    }
    drop(lifecycle_resources.take());
    match recovery_disposition {
        continuity::RecoveryDisposition::Replace => {
            let identity = executable_identity.as_ref().map_err(|error| {
                continuity::unavailable(*error, &executable, state_root.as_deref(), session_id)
            })?;
            continuity::replace(
                &mut input_recovery,
                &executable,
                identity,
                &cwd,
                state_root.as_deref(),
                session_id,
            )
        }
        continuity::RecoveryDisposition::FailClosed(reason) => Err(continuity::stalled(
            reason,
            &executable,
            state_root.as_deref(),
            session_id,
        )),
        continuity::RecoveryDisposition::None => restart::resume_after_update(
            installation.as_ref(),
            requested_restart.as_ref(),
            session_id,
            state_root.as_deref(),
            previous_instance_id,
        ),
    }
}

pub(super) fn supports_true_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::env::var("COLORTERM")
        .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "truecolor" | "24bit"))
        || std::env::var("TERM").is_ok_and(|value| value.to_ascii_lowercase().contains("direct"))
}
