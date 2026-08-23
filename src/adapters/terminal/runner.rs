//! Bounded UI and persistence lane composition.

use std::{
    collections::BTreeMap,
    io::{IsTerminal, Stdout, Write, stdout},
    path::PathBuf,
    sync::mpsc::TryRecvError,
    thread,
    time::Duration,
};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    adapters::{
        control::{ControlEnvelope, ControlServer},
        editor::RopeEditorFactory,
        runtime::{FileSchemaLease, FileSessionLease, SystemClock, SystemIdGenerator},
        sqlite::SqliteStore,
    },
    application::{AppState, ControlReplay, Effect, FailureCode, match_control_replay},
    domain::{OperationSequence, SessionId, ThoughtId},
    ports::{
        control::{ControlReceipt, ControlResult},
        store::StoreError,
    },
    ui::{BoardApp, Theme, render},
};

use super::{
    TerminalError,
    control::{CrosstermControl, TerminalGuard, TerminationGuard},
    external::{ExternalLane, ExternalResult},
    input::{InputLane, InputMessage},
    integration::integration_context,
    persistence::{PersistenceLane, PersistenceResult},
};

/// Concrete runtime pieces retained for one interactive session.
pub(crate) struct TerminalResources {
    pub(crate) state: AppState,
    pub(crate) store: SqliteStore,
    pub(crate) clock: SystemClock,
    pub(crate) ids: SystemIdGenerator,
    pub(crate) session_lease: FileSessionLease,
    pub(crate) schema_lease: FileSchemaLease,
    pub(crate) settings: crate::ui::UiSettings,
    pub(crate) recovery_directory: PathBuf,
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
    control: &'a ControlServer,
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
        clock,
        mut ids,
        mut session_lease,
        schema_lease,
        settings,
        recovery_directory,
    } = resources;
    let session_id = state.board.session.id;
    let endpoint = session_lease
        .control_endpoint()
        .ok_or(crate::ports::control::ControlError::Unsupported)?
        .to_owned();
    let control = ControlServer::spawn(&endpoint)?;
    session_lease.publish_control()?;
    let guard = TerminalGuard::enter(CrosstermControl)?;
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let input = InputLane::spawn();
    let persistence = PersistenceLane::spawn(store);
    let external = ExternalLane::spawn(recovery_directory);
    let theme = Theme::resolve(settings.theme, supports_true_color());
    let mut app = BoardApp::with_settings(state, settings, RopeEditorFactory);
    let lanes = WorkerLanes {
        input: &input,
        persistence: &persistence,
        external: &external,
        control: &control,
        termination: &termination,
    };
    let run_result = drive(&mut terminal, &mut app, &lanes, &mut ids, clock, theme);
    let control_result = control.stop();
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

fn drive(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
    theme: Theme,
) -> Result<(), TerminalError> {
    let mut pending = PendingWork::default();
    enqueue_effects(app, lanes, BoardApp::discover_agents(), &mut pending)?;
    let mut redraw = true;
    loop {
        if lanes.termination.requested() {
            app.quit = true;
        }
        redraw |= drain_persistence(app, lanes.persistence, &mut pending)?;
        redraw |= drain_external(app, lanes, &mut pending, ids, clock)?;
        redraw |= drain_control(app, lanes, &mut pending, clock)?;
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

fn enqueue_effects(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    effects: Vec<Effect>,
    pending: &mut PendingWork,
) -> Result<(), TerminalError> {
    for effect in effects {
        if let Some(batch) = effect.persistence_batch() {
            let sequence = batch
                .sequence()
                .ok_or(TerminalError::Worker("mutable batch lacks sequence"))?;
            if let Err(error) = lanes.persistence.commit(batch) {
                app.acknowledge_persistence(sequence, false);
                return Err(error);
            }
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::RetryPersistence { sequence } = effect {
            lanes.persistence.retry(sequence)?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if let Effect::StoreIntegrationContext {
            session_id,
            target,
            verified_at,
        } = effect
        {
            lanes.persistence.metadata(
                crate::ports::store::OperationBatch::IntegrationContext {
                    session_id,
                    context: Some(integration_context(&target, verified_at)),
                },
            )?;
            pending.persistence = pending.persistence.saturating_add(1);
        } else if lanes.external.send(&effect)? {
            pending.external = pending.external.saturating_add(1);
        } else if let Effect::Notify { code } = effect {
            app.notify(code);
        }
    }
    Ok(())
}

fn drain_persistence(
    app: &mut BoardApp,
    persistence: &PersistenceLane,
    pending: &mut PendingWork,
) -> Result<bool, TerminalError> {
    let mut changed = false;
    loop {
        match persistence.receiver.try_recv() {
            Ok(PersistenceResult::Sequenced { sequence, result }) => {
                changed = true;
                pending.persistence = pending.persistence.saturating_sub(1);
                let succeeded = match result {
                    Ok(receipt) => {
                        complete_control(
                            pending,
                            sequence,
                            ControlResult::Accepted(ControlReceipt {
                                thought_id: pending
                                    .controls
                                    .get(&sequence)
                                    .and_then(|control| control.thought_id),
                                durable: receipt,
                            }),
                        );
                        true
                    }
                    Err(error) => {
                        complete_control(
                            pending,
                            sequence,
                            ControlResult::Rejected {
                                code: storage_error_code(&error).to_owned(),
                                message: error.to_string(),
                            },
                        );
                        app.status =
                            Some(format!("{error}; press r to retry or w to export recovery"));
                        false
                    }
                };
                app.acknowledge_persistence(sequence, succeeded);
            }
            Ok(PersistenceResult::Metadata { result }) => {
                changed = true;
                pending.persistence = pending.persistence.saturating_sub(1);
                if let Err(error) = result {
                    app.status = Some(format!(
                        "submission accepted, but integration context was not saved: {error}"
                    ));
                }
            }
            Err(TryRecvError::Empty) => return Ok(changed),
            Err(TryRecvError::Disconnected) if pending.persistence == 0 => return Ok(changed),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker(
                    "persistence result lane disconnected",
                ));
            }
        }
    }
}

fn complete_control(pending: &mut PendingWork, sequence: OperationSequence, result: ControlResult) {
    if let Some(control) = pending.controls.remove(&sequence) {
        control.envelope.respond(result);
    }
}

fn drain_control(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    clock: SystemClock,
) -> Result<bool, TerminalError> {
    let mut changed = false;
    loop {
        let envelope = match lanes.control.receiver.try_recv() {
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
            ExternalResult::Read { request_id, result } => app.complete_clipboard_read(
                request_id,
                result.map_err(|_| FailureCode::ClipboardFailed),
                ids,
                &clock,
            ),
            ExternalResult::Exported { request_id, result } => {
                app.complete_recovery_export(request_id, result.map_err(|error| error.to_string()))
            }
            ExternalResult::AgentsDiscovered(result) => {
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

fn write_osc52(sequence: &[u8]) -> std::io::Result<()> {
    let output = stdout();
    let mut writer = output.lock();
    writer.write_all(sequence)?;
    writer.flush()
}
