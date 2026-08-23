//! Bounded UI and persistence lane composition.

use std::{
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
        runtime::{FileSchemaLease, FileSessionLease, SystemClock, SystemIdGenerator},
        sqlite::SqliteStore,
    },
    application::{AppState, Effect, FailureCode},
    domain::SessionId,
    ui::{BoardApp, Theme, render},
};

use super::{
    TerminalError,
    control::{CrosstermControl, TerminalGuard, TerminationGuard},
    external::{ExternalLane, ExternalResult},
    input::{InputLane, InputMessage},
    persistence::PersistenceLane,
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
    termination: &'a TerminationGuard,
}

#[derive(Default)]
struct PendingWork {
    persistence: usize,
    external: usize,
}

impl PendingWork {
    const fn is_empty(&self) -> bool {
        self.persistence == 0 && self.external == 0
    }
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
        session_lease,
        schema_lease,
        settings,
        recovery_directory,
    } = resources;
    let session_id = state.board.session.id;
    let guard = TerminalGuard::enter(CrosstermControl)?;
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let input = InputLane::spawn();
    let persistence = PersistenceLane::spawn(store);
    let external = ExternalLane::spawn(recovery_directory);
    let theme = Theme::resolve(settings.theme, supports_true_color());
    let mut app = BoardApp::with_settings(state, settings);
    let lanes = WorkerLanes {
        input: &input,
        persistence: &persistence,
        external: &external,
        termination: &termination,
    };
    let run_result = drive(&mut terminal, &mut app, &lanes, &mut ids, clock, theme);
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
    loop {
        if lanes.termination.requested() {
            app.quit = true;
        }
        drain_persistence(app, lanes.persistence, &mut pending)?;
        drain_external(app, lanes, &mut pending, ids, clock)?;
        terminal.draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(frame, app, &layout, &theme);
        })?;
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
) -> Result<(), TerminalError> {
    loop {
        match persistence.receiver.try_recv() {
            Ok(outcome) => {
                pending.persistence = pending.persistence.saturating_sub(1);
                let succeeded = outcome.result.is_ok();
                if let Err(error) = outcome.result {
                    app.status = Some(format!("{error}; press r to retry or w to export recovery"));
                }
                app.acknowledge_persistence(outcome.sequence, succeeded);
            }
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) if pending.persistence == 0 => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker(
                    "persistence result lane disconnected",
                ));
            }
        }
    }
}

fn drain_external(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    ids: &mut SystemIdGenerator,
    clock: SystemClock,
) -> Result<(), TerminalError> {
    loop {
        let result = match lanes.external.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) if pending.external == 0 => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker("external result lane disconnected"));
            }
        };
        pending.external = pending.external.saturating_sub(1);
        let effects = match result {
            ExternalResult::Written { request_id, result } => {
                let succeeded = match result {
                    Ok(crate::ports::clipboard::ClipboardWrite::Native) => true,
                    Ok(crate::ports::clipboard::ClipboardWrite::Osc52(sequence)) => {
                        write_osc52(&sequence).is_ok()
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
