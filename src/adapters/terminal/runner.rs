//! Bounded UI and persistence lane composition.

use std::{
    io::{IsTerminal, Stdout, stdout},
    sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    thread::{self, JoinHandle},
    time::Duration,
};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    adapters::{
        runtime::{FileSchemaLease, FileSessionLease, SystemClock, SystemIdGenerator},
        sqlite::SqliteStore,
    },
    application::{AppState, Effect},
    domain::{OperationSequence, SessionId},
    ports::store::{OperationBatch, Store, StoreError},
    ui::{BoardApp, Theme, render},
};

use super::{
    TerminalError,
    control::{CrosstermControl, TerminalGuard, TerminationGuard},
    input::{InputLane, InputMessage},
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

struct PersistenceResult {
    sequence: OperationSequence,
    result: Result<(), StoreError>,
}

struct PersistenceLane {
    sender: Option<SyncSender<OperationBatch>>,
    receiver: Receiver<PersistenceResult>,
    handle: Option<JoinHandle<()>>,
}

struct WorkerLanes<'a> {
    input: &'a InputLane,
    persistence: &'a PersistenceLane,
    termination: &'a TerminationGuard,
}

impl PersistenceLane {
    fn spawn(store: SqliteStore) -> Self {
        let (request_sender, request_receiver) = sync_channel(64);
        let (result_sender, result_receiver) = sync_channel(64);
        let handle =
            thread::spawn(move || persistence_loop(store, &request_receiver, &result_sender));
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
        }
    }

    fn send(&self, batch: OperationBatch) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("persistence lane is closed"))?
            .send(batch)
            .map_err(|_| TerminalError::Worker("persistence lane disconnected"))
    }

    fn stop(mut self) -> Result<(), TerminalError> {
        drop(self.sender.take());
        match self.handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(TerminalError::Worker("persistence lane panicked")),
        }
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
    } = resources;
    let session_id = state.board.session.id;
    let guard = TerminalGuard::enter(CrosstermControl)?;
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let input = InputLane::spawn();
    let persistence = PersistenceLane::spawn(store);
    let theme = Theme::resolve(settings.theme, supports_true_color());
    let mut app = BoardApp::with_settings(state, settings);
    let lanes = WorkerLanes {
        input: &input,
        persistence: &persistence,
        termination: &termination,
    };
    let run_result = drive(&mut terminal, &mut app, &lanes, &mut ids, clock, theme);
    let input_result = input
        .stop()
        .map_err(|_| TerminalError::Worker("input lane panicked"));
    let persistence_result = persistence.stop();
    drop(terminal);
    let restoration_result = guard.finish();
    drop((session_lease, schema_lease));
    run_result?;
    input_result?;
    persistence_result?;
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
    let mut awaiting = 0_usize;
    loop {
        if lanes.termination.requested() {
            app.quit = true;
        }
        drain_persistence(app, lanes.persistence, &mut awaiting)?;
        terminal.draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(frame, app, &layout, &theme);
        })?;
        if app.quit && awaiting == 0 {
            return Ok(());
        }
        if app.quit {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        match lanes.input.receiver.recv_timeout(Duration::from_millis(30)) {
            Ok(InputMessage::Event(event)) => {
                let effects = app.handle(event, ids, &clock);
                enqueue_effects(app, lanes.persistence, effects, &mut awaiting)?;
            }
            Ok(InputMessage::Failed(message)) => return Err(TerminalError::Io(message)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(TerminalError::Worker("input lane disconnected"));
            }
        }
    }
}

fn supports_true_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::env::var("COLORTERM")
        .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "truecolor" | "24bit"))
        || std::env::var("TERM").is_ok_and(|value| value.to_ascii_lowercase().contains("direct"))
}

fn enqueue_effects(
    app: &mut BoardApp,
    persistence: &PersistenceLane,
    effects: Vec<Effect>,
    awaiting: &mut usize,
) -> Result<(), TerminalError> {
    for effect in effects {
        if let Some(batch) = effect.persistence_batch() {
            let sequence = batch
                .sequence()
                .ok_or(TerminalError::Worker("mutable batch lacks sequence"))?;
            if let Err(error) = persistence.send(batch) {
                app.acknowledge_persistence(sequence, false);
                return Err(error);
            }
            *awaiting = awaiting.saturating_add(1);
        }
    }
    Ok(())
}

fn drain_persistence(
    app: &mut BoardApp,
    persistence: &PersistenceLane,
    awaiting: &mut usize,
) -> Result<(), TerminalError> {
    loop {
        match persistence.receiver.try_recv() {
            Ok(outcome) => {
                *awaiting = awaiting.saturating_sub(1);
                let succeeded = outcome.result.is_ok();
                if let Err(error) = outcome.result {
                    app.status = Some(error.to_string());
                }
                app.acknowledge_persistence(outcome.sequence, succeeded);
            }
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) if *awaiting == 0 => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                return Err(TerminalError::Worker(
                    "persistence result lane disconnected",
                ));
            }
        }
    }
}

fn persistence_loop(
    mut store: SqliteStore,
    requests: &Receiver<OperationBatch>,
    results: &SyncSender<PersistenceResult>,
) {
    while let Ok(batch) = requests.recv() {
        let Some(sequence) = batch.sequence() else {
            continue;
        };
        let result = store.commit(&batch).map(|_receipt| ());
        if results
            .send(PersistenceResult { sequence, result })
            .is_err()
        {
            return;
        }
    }
}
