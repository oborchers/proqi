//! Bounded UI and persistence lane composition.

mod diagnostics;
mod durability;
mod external_results;
mod fairness;
pub(super) mod finish;
mod heartbeat;
mod owned_lanes;
mod owner_control;
mod update_results;

use std::{
    collections::BTreeMap,
    io::{IsTerminal, Stdout, stdin, stdout},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    adapters::{
        control::{ControlEnvelope, ControlServer},
        editor::RopeEditorFactory,
        runtime::{
            FileRuntimeCoordinator, FileSchemaLease, FileSessionLease, SystemClock,
            SystemIdGenerator,
        },
        sqlite::SqliteStore,
    },
    application::AppState,
    domain::{OperationSequence, RequestId, SessionId, ThoughtId},
    ports::{
        environment::Clock as _,
        runtime::InstanceInfo,
        store::{Store as _, StoreError},
    },
    ui::{BoardApp, Theme, UiInput, UiKey, render},
};

use super::{
    TerminalError,
    control::{CrosstermControl, PanicHookGuard, TerminalGuard, TerminationGuard},
    external::ExternalLane,
    input::{InputLane, InputMessage},
    persistence::PersistenceLane,
};

use durability::{drain_persistence, enqueue_effects};
use heartbeat::PaneHeartbeat;
use owned_lanes::OwnedLanes;

/// Concrete runtime pieces retained for one interactive session.
pub(crate) struct TerminalResources {
    pub(crate) state: AppState,
    pub(crate) store: SqliteStore,
    pub(crate) coordinator: FileRuntimeCoordinator,
    pub(crate) clock: SystemClock,
    pub(crate) ids: SystemIdGenerator,
    pub(crate) cwd: PathBuf,
    pub(crate) session_lease: FileSessionLease,
    pub(crate) schema_lease: FileSchemaLease,
    pub(crate) settings: crate::ui::UiSettings,
    pub(crate) recovery_directory: PathBuf,
    pub(crate) attachment_directory: PathBuf,
    pub(crate) installation: Option<crate::domain::Installation>,
    pub(crate) cache_directory: PathBuf,
    pub(crate) state_root: Option<PathBuf>,
}

/// Refuse an interactive launch before it creates or opens durable state.
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
    pub(super) input: &'a InputLane,
    pub(super) persistence: &'a PersistenceLane,
    pub(super) external: &'a ExternalLane,
    pub(super) control: Option<&'a ControlServer>,
    pub(super) update: &'a super::update_lane::UpdateLane,
    pub(super) termination: &'a TerminationGuard,
    pub(super) instance: &'a InstanceInfo,
    pub(super) cancellation: &'a crate::adapters::process::CancellationFlag,
}

#[derive(Default)]
pub(super) struct PendingWork {
    pub(super) persistence: usize,
    pub(super) external: usize,
    pub(super) controls: BTreeMap<OperationSequence, PendingControl>,
    pub(super) control_lookups: BTreeMap<RequestId, ControlEnvelope>,
    pub(super) update_prepares: BTreeMap<RequestId, ControlEnvelope>,
    pub(super) update: usize,
}

impl PendingWork {
    fn is_empty(&self) -> bool {
        self.persistence == 0
            && self.external == 0
            && self.controls.is_empty()
            && self.control_lookups.is_empty()
            && self.update_prepares.is_empty()
            && self.update == 0
    }
}

pub(super) struct PendingControl {
    pub(super) envelope: ControlEnvelope,
    pub(super) thought_id: Option<ThoughtId>,
}

/// Run a leased session until a clean user exit.
///
/// # Errors
///
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
    } = resources;
    let session_id = state.board.session.id;
    store.recover_submissions(session_id, clock.now())?;
    let (control, control_warning) = start_optional_control(&mut session_lease);
    let (theme, guard) = enter_terminal(settings.theme, settings.keyboard_enhancement)?;
    let panic_hook = PanicHookGuard::install();
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let presentation_source = format!("proqi-{}", session_lease.info().instance_id);
    let mut owned = spawn_lanes(
        control,
        store,
        coordinator,
        cwd,
        recovery_directory,
        attachment_directory,
        presentation_source,
        cache_directory,
        installation.clone(),
    );
    let mut pane_heartbeat = None;
    let shutdown = super::supervisor::ShutdownCoordinator::default();
    let check_for_updates = settings.check_for_updates;
    let mut app = BoardApp::with_settings(state, settings, RopeEditorFactory);
    if let Some(warning) = control_warning {
        app.set_warning(warning);
    }
    let lanes = WorkerLanes {
        input: &owned.input,
        persistence: &owned.persistence,
        external: &owned.external,
        control: owned.control.as_ref(),
        update: &owned.update,
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
        lane_results
            .into_iter()
            .chain([control_result, restoration_result]),
    )?;
    resume_after_update(
        installation.as_ref(),
        requested_restart.as_ref(),
        session_id,
        state_root.as_deref(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "composition root owns explicit adapter inputs"
)]
fn spawn_lanes(
    control: Option<ControlServer>,
    store: SqliteStore,
    coordinator: FileRuntimeCoordinator,
    cwd: PathBuf,
    recovery_directory: PathBuf,
    attachment_directory: PathBuf,
    presentation_source: String,
    cache_directory: PathBuf,
    installation: Option<crate::domain::Installation>,
) -> OwnedLanes {
    let cancellation = crate::adapters::process::CancellationFlag::default();
    OwnedLanes {
        control,
        input: InputLane::spawn(),
        persistence: PersistenceLane::spawn_with_runtime(store, coordinator.clone(), cwd),
        external: ExternalLane::spawn(
            recovery_directory,
            attachment_directory,
            presentation_source,
            cancellation.clone(),
        ),
        update: super::update_lane::UpdateLane::spawn(
            cache_directory,
            installation,
            coordinator,
            cancellation.clone(),
        ),
        cancellation,
    }
}

fn resume_after_update(
    installation: Option<&crate::domain::Installation>,
    requested: Option<&crate::domain::StableVersion>,
    session_id: SessionId,
    state_root: Option<&std::path::Path>,
) -> Result<SessionId, TerminalError> {
    requested.map_or(Ok(session_id), |version| {
        replace_after_cleanup(installation, version, session_id, state_root)
    })
}

fn replace_after_cleanup(
    installation: Option<&crate::domain::Installation>,
    expected: &crate::domain::StableVersion,
    session_id: SessionId,
    state_root: Option<&std::path::Path>,
) -> Result<SessionId, TerminalError> {
    use crate::ports::update::{InstallDetector as _, ProcessReplacer as _};

    let installation = installation.ok_or_else(|| {
        TerminalError::Io("update restart lacks a verified installation context".to_owned())
    })?;
    if installation.kind != crate::domain::InstallationKind::HomebrewFormula {
        return Err(TerminalError::Io(
            "automatic restart is available only for verified Homebrew installations".to_owned(),
        ));
    }
    let active = installation
        .restart_executable
        .as_ref()
        .ok_or_else(|| TerminalError::Io("Homebrew active executable is unavailable".to_owned()))?;
    let detected = crate::adapters::update::SystemInstallDetector::for_executable(active.clone())
        .detect()
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    if detected.kind != crate::domain::InstallationKind::HomebrewFormula
        || detected.identity != installation.identity
    {
        return Err(TerminalError::Io(
            "updated executable does not belong to this Homebrew installation".to_owned(),
        ));
    }
    let mut runner = crate::adapters::process::SystemProcessRunner::default();
    crate::adapters::update::verify_installed_version(&mut runner, &detected.executable, expected)
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    crate::adapters::process::SystemProcessReplacer
        .replace(&detected.executable, session_id, state_root)
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    Ok(session_id)
}

fn start_optional_control(
    session_lease: &mut FileSessionLease,
) -> (Option<ControlServer>, Option<String>) {
    let Some(endpoint) = session_lease.control_endpoint() else {
        return (
            None,
            Some("active-session CLI forwarding is unavailable on this platform".to_owned()),
        );
    };
    let server = match ControlServer::spawn(endpoint) {
        Ok(server) => server,
        Err(error) => {
            return (
                None,
                Some(format!(
                    "active-session CLI forwarding unavailable: {error}"
                )),
            );
        }
    };
    if let Err(error) = session_lease.publish_control() {
        let _stopped = server.stop();
        return (
            None,
            Some(format!(
                "active-session CLI forwarding unavailable: {error}"
            )),
        );
    }
    (Some(server), None)
}

fn enter_terminal(
    preference: crate::ui::ThemePreference,
    keyboard: crate::ui::KeyboardEnhancement,
) -> Result<(Theme, TerminalGuard<CrosstermControl>), TerminalError> {
    let theme = super::palette::resolve(preference, supports_true_color());
    let guard = TerminalGuard::enter(CrosstermControl::new(keyboard))?;
    Ok((theme, guard))
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
    let mut edit_generation = app.edit_generation();
    let mut edit_deadline = None;
    let mut agent_deadline = None;
    let mut termination_seen = false;
    enqueue_effects(app, lanes, BoardApp::discover_agents(), &mut pending)?;
    let mut redraw = true;
    loop {
        if lanes.termination.requested() && !termination_seen {
            termination_seen = true;
            let _deadline = shutdown.request();
            lanes.cancellation.cancel();
            if let Some(control) = lanes.control {
                control.request_stop();
            }
            let effects = app.handle(UiInput::Key(UiKey::Quit), ids, &clock);
            enqueue_effects(app, lanes, effects, &mut pending)?;
        }
        let (workers_changed, worker_backlog) =
            drain_workers(app, lanes, &mut pending, ids, clock, pane_heartbeat)?;
        redraw |= workers_changed || app.expire_update_barrier(clock.now());
        if app.edit_generation() != edit_generation {
            edit_generation = app.edit_generation();
            edit_deadline = app
                .has_pending_edit()
                .then(|| Instant::now() + Duration::from_millis(250));
        }
        if edit_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let effects = app.flush_pending_edit(ids, &clock);
            enqueue_effects(app, lanes, effects, &mut pending)?;
            edit_deadline = None;
            redraw = true;
        }
        if agent_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            enqueue_effects(app, lanes, BoardApp::discover_agents(), &mut pending)?;
            agent_deadline = None;
        }
        if let Some(heartbeat) = pane_heartbeat.as_mut() {
            let _refreshed = heartbeat.refresh_if_due(lanes.external);
        }
        if redraw {
            terminal.draw(|frame| {
                let layout = app.prepare_frame(frame.area());
                render(frame, app, &layout, &theme);
            })?;
            app.arm_update_prompt();
            redraw = false;
        }
        if app.quit {
            let deadline = shutdown.request();
            lanes.cancellation.cancel();
            if let Some(control) = lanes.control {
                control.request_stop();
            }
            if pending.is_empty() {
                return Ok(());
            }
            if deadline.expired() {
                return Err(TerminalError::Worker(
                    "runtime shutdown exceeded its shared deadline",
                ));
            }
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
                if !app.accept_update_input(sequence) {
                    continue;
                }
                if matches!(event, UiInput::Resize { .. }) {
                    agent_deadline = Some(Instant::now() + Duration::from_millis(250));
                }
                let effects = app.handle(event, ids, &clock);
                enqueue_effects(app, lanes, effects, &mut pending)?;
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
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
) -> Result<(bool, bool), TerminalError> {
    let persistence = drain_persistence(app, lanes, pending, ids, &clock)?;
    let external = external_results::drain(app, lanes, pending, ids, clock, pane_heartbeat)?;
    let control = owner_control::drain(app, lanes, pending, ids, clock)?;
    let update = update_results::drain(app, lanes, pending)?;
    let changed = persistence.changed || external.changed || control.changed || update.changed;
    let backlog = persistence.budget_exhausted
        || external.budget_exhausted
        || control.budget_exhausted
        || update.budget_exhausted;
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

pub(super) const fn storage_error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::Busy => "storage_busy",
        StoreError::DiskFull => "storage_full",
        StoreError::RecoveryCapacity => "recovery_capacity",
        _ => "storage_failed",
    }
}
