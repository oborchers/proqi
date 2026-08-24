//! Bounded UI and persistence lane composition.

mod durability;
mod external_results;
mod fairness;
mod heartbeat;
mod owner_control;

use std::{
    collections::BTreeMap,
    io::{IsTerminal, Stdout, stdout},
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
    ports::store::StoreError,
    ui::{BoardApp, Theme, UiInput, UiKey, render},
};

use super::{
    TerminalError,
    control::{CrosstermControl, TerminalGuard, TerminationGuard},
    external::ExternalLane,
    input::{InputLane, InputMessage},
    persistence::PersistenceLane,
};

use durability::{drain_persistence, enqueue_effects};
use heartbeat::PaneHeartbeat;

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
}

/// Refuse an interactive launch before it creates or opens durable state.
pub(crate) fn require_interactive() -> Result<(), TerminalError> {
    if stdout().is_terminal() {
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
    pub(super) termination: &'a TerminationGuard,
}

#[derive(Default)]
pub(super) struct PendingWork {
    pub(super) persistence: usize,
    pub(super) external: usize,
    pub(super) controls: BTreeMap<OperationSequence, PendingControl>,
    pub(super) control_lookups: BTreeMap<RequestId, ControlEnvelope>,
}

impl PendingWork {
    fn is_empty(&self) -> bool {
        self.persistence == 0
            && self.external == 0
            && self.controls.is_empty()
            && self.control_lookups.is_empty()
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
pub(crate) fn run(resources: TerminalResources) -> Result<SessionId, TerminalError> {
    require_interactive()?;
    let TerminalResources {
        state,
        store,
        coordinator,
        clock,
        mut ids,
        cwd,
        mut session_lease,
        schema_lease,
        settings,
        recovery_directory,
        attachment_directory,
    } = resources;
    let session_id = state.board.session.id;
    let (control, control_warning) = start_optional_control(&mut session_lease);
    let theme = super::palette::resolve(settings.theme, supports_true_color());
    let guard = TerminalGuard::enter(CrosstermControl)?;
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let input = InputLane::spawn();
    let persistence = PersistenceLane::spawn_with_runtime(store, coordinator, cwd);
    let presentation_source = format!("proqi-{}", session_lease.info().instance_id);
    let external = ExternalLane::spawn(
        recovery_directory,
        attachment_directory,
        presentation_source,
    );
    let mut pane_heartbeat = None;
    let mut app = BoardApp::with_settings(state, settings, RopeEditorFactory);
    if let Some(warning) = control_warning {
        app.set_warning(warning);
    }
    let lanes = WorkerLanes {
        input: &input,
        persistence: &persistence,
        external: &external,
        control: control.as_ref(),
        termination: &termination,
    };
    let run_result = drive(
        &mut terminal,
        &mut app,
        &lanes,
        &mut ids,
        clock,
        theme,
        &mut pane_heartbeat,
    );
    if let Some(heartbeat) = pane_heartbeat.as_mut() {
        let _cleared = heartbeat.clear(&external);
    }
    let control_result = control.map_or(Ok(()), ControlServer::stop);
    let input_result = input
        .stop()
        .map_err(|_| TerminalError::Worker("input lane panicked"));
    let persistence_result = persistence.stop();
    let external_result = external.stop();
    drop(terminal);
    let restoration_result = guard.finish();
    drop((session_lease, schema_lease));
    run_result?;
    input_result?;
    persistence_result?;
    external_result?;
    control_result?;
    restoration_result?;
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

fn drive(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    theme: Theme,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
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
            let effects = app.handle(UiInput::Key(UiKey::Quit), ids, &clock);
            enqueue_effects(app, lanes, effects, &mut pending)?;
        }
        let persistence = drain_persistence(app, lanes, &mut pending, ids, &clock)?;
        let external =
            external_results::drain(app, lanes, &mut pending, ids, clock, pane_heartbeat)?;
        let control = owner_control::drain(app, lanes, &mut pending, ids, clock)?;
        redraw |= persistence.changed || external.changed || control.changed;
        let worker_backlog =
            persistence.budget_exhausted || external.budget_exhausted || control.budget_exhausted;
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
            redraw = false;
        }
        if app.quit && pending.is_empty() {
            return Ok(());
        }
        if app.quit {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        let input_wait = if worker_backlog {
            Duration::ZERO
        } else {
            Duration::from_millis(30)
        };
        match lanes.input.receiver.recv_timeout(input_wait) {
            Ok(InputMessage::Event(event)) => {
                if matches!(event, UiInput::Resize { .. }) {
                    agent_deadline = Some(Instant::now() + Duration::from_millis(250));
                }
                let effects = app.handle(event, ids, &clock);
                enqueue_effects(app, lanes, effects, &mut pending)?;
                redraw = true;
            }
            Ok(InputMessage::Failed(message)) => return Err(TerminalError::Io(message)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(TerminalError::Worker("input lane disconnected"));
            }
        }
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

pub(super) const fn storage_error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::Busy => "storage_busy",
        StoreError::DiskFull => "storage_full",
        _ => "storage_failed",
    }
}
