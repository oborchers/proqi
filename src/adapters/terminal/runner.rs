//! Bounded UI and persistence lane composition.

mod durability;
mod heartbeat;

use std::{
    collections::BTreeMap,
    io::{IsTerminal, Stdout, Write, stdout},
    path::PathBuf,
    sync::mpsc::TryRecvError,
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
    application::{AppState, ControlReplay, Effect, FailureCode, match_control_replay},
    domain::{OperationSequence, SessionId, ThoughtId},
    ports::{control::ControlResult, store::StoreError},
    ui::{BoardApp, Theme, UiInput, UiKey, render},
};

use super::{
    TerminalError,
    control::{CrosstermControl, TerminalGuard, TerminationGuard},
    external::{ExternalLane, ExternalResult},
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

struct WorkerLanes<'a> {
    input: &'a InputLane,
    persistence: &'a PersistenceLane,
    external: &'a ExternalLane,
    control: Option<&'a ControlServer>,
    termination: &'a TerminationGuard,
}

#[derive(Default)]
struct PendingWork {
    persistence: usize,
    external: usize,
    controls: BTreeMap<OperationSequence, PendingControl>,
}

impl PendingWork {
    fn is_empty(&self) -> bool {
        self.persistence == 0 && self.external == 0 && self.controls.is_empty()
    }
}

struct PendingControl {
    envelope: ControlEnvelope,
    thought_id: Option<ThoughtId>,
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
        redraw |= drain_persistence(app, lanes, &mut pending, ids, &clock)?;
        redraw |= drain_external(app, lanes, &mut pending, ids, clock, pane_heartbeat)?;
        redraw |= drain_control(app, lanes, &mut pending, ids, clock)?;
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
        match lanes.input.receiver.recv_timeout(Duration::from_millis(30)) {
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

fn drain_control(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
) -> Result<bool, TerminalError> {
    let Some(control) = lanes.control else {
        return Ok(false);
    };
    let mut changed = false;
    loop {
        let envelope = match control.receiver.try_recv() {
            Ok(envelope) => envelope,
            Err(TryRecvError::Empty) => return Ok(changed),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker("control request lane disconnected"));
            }
        };
        if envelope.request.session_id != app.state.board.session.id {
            envelope.respond(ControlResult::Rejected {
                code: "wrong_session".to_owned(),
                message: "request does not address the active owner session".to_owned(),
            });
            continue;
        }
        let edit_effects = app.flush_pending_edit(ids, &clock);
        enqueue_effects(app, lanes, edit_effects, pending)?;
        match lanes
            .persistence
            .lookup(envelope.request.mutation.operation_id())
        {
            Ok(Some(existing)) => {
                let result = match match_control_replay(
                    &existing,
                    envelope.request.session_id,
                    &envelope.request.mutation,
                ) {
                    ControlReplay::Accepted(receipt) => ControlResult::Accepted(receipt),
                    ControlReplay::Conflict => ControlResult::Rejected {
                        code: "idempotency_conflict".to_owned(),
                        message: "operation identity belongs to another request".to_owned(),
                    },
                };
                envelope.respond(result);
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                let code = match &error {
                    TerminalError::Store(source) => storage_error_code(source),
                    _ => "storage_failed",
                };
                envelope.respond(ControlResult::Rejected {
                    code: code.to_owned(),
                    message: error.to_string(),
                });
                continue;
            }
        }
        let thought_id = envelope.request.mutation.thought_id();
        match app.handle_control(&envelope.request.mutation, &clock) {
            Ok(effects) => {
                queue_control_effect(app, lanes, pending, envelope, thought_id, &effects)?;
                changed = true;
            }
            Err(error) => envelope.respond(ControlResult::Rejected {
                code: error.code().as_str().to_owned(),
                message: error.to_string(),
            }),
        }
    }
}

const fn storage_error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::Busy => "storage_busy",
        StoreError::DiskFull => "storage_full",
        _ => "storage_failed",
    }
}

fn queue_control_effect(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    envelope: ControlEnvelope,
    thought_id: Option<ThoughtId>,
    effects: &[Effect],
) -> Result<(), TerminalError> {
    let [effect] = effects else {
        envelope.respond(ControlResult::Rejected {
            code: "no_durable_mutation".to_owned(),
            message: "request produced no durable mutation".to_owned(),
        });
        return Ok(());
    };
    let batch = effect
        .persistence_batch()
        .ok_or(TerminalError::Worker("control mutation lacked persistence"))?;
    let sequence = batch
        .sequence()
        .ok_or(TerminalError::Worker("control mutation lacked sequence"))?;
    if let Err(error) = lanes.persistence.commit(batch) {
        app.acknowledge_persistence(sequence, false);
        envelope.respond(ControlResult::Rejected {
            code: "storage_failed".to_owned(),
            message: error.to_string(),
        });
        return Err(error);
    }
    pending.persistence = pending.persistence.saturating_add(1);
    pending.controls.insert(
        sequence,
        PendingControl {
            envelope,
            thought_id,
        },
    );
    Ok(())
}

fn drain_external(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    pane_heartbeat: &mut Option<PaneHeartbeat>,
) -> Result<bool, TerminalError> {
    let mut changed = false;
    loop {
        let result = match lanes.external.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(changed),
            Err(TryRecvError::Disconnected) if pending.external == 0 => return Ok(changed),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker("external result lane disconnected"));
            }
        };
        changed = true;
        pending.external = pending.external.saturating_sub(1);
        let effects = match result {
            ExternalResult::Written {
                request_id,
                intent,
                result,
            } => {
                let succeeded = match result {
                    Ok(crate::ports::clipboard::ClipboardWrite::Native) => true,
                    Ok(crate::ports::clipboard::ClipboardWrite::Osc52(sequence)) => {
                        let emitted = write_osc52(&sequence).is_ok();
                        emitted && intent == crate::application::ClipboardIntent::Copy
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
                publish_discovered_identity(pane_heartbeat, pane_id, lanes.external);
                app.complete_agent_discovery(result);
                Vec::new()
            }
            ExternalResult::AgentSubmitted {
                submission_id,
                result,
            } => app.complete_submission(submission_id, *result),
        };
        enqueue_effects(app, lanes, effects, pending)?;
    }
}

fn publish_discovered_identity(
    heartbeat: &mut Option<PaneHeartbeat>,
    pane_id: Option<String>,
    external: &ExternalLane,
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
    let output = stdout();
    let mut writer = output.lock();
    writer.write_all(sequence)?;
    writer.flush()
}
