//! Crossterm event normalization and lossless input delivery.

use std::{
    fmt, io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::ui::UiInput;

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, join_before},
};

use translation::{translate, translate_key};

mod translation;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum InputFailure {
    EndOfFile,
    TerminalRevoked,
    Unresponsive,
    Io(String),
}

impl fmt::Display for InputFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfFile => formatter.write_str("terminal input reached end of file"),
            Self::TerminalRevoked => formatter.write_str("terminal input was revoked"),
            Self::Unresponsive => formatter.write_str("terminal input became unresponsive"),
            Self::Io(message) => write!(formatter, "terminal input failed: {message}"),
        }
    }
}

const SOURCE_POLL_INTERVAL: Duration = Duration::from_millis(40);
const MONITOR_INTERVAL: Duration = Duration::from_millis(50);
const SOURCE_STALL_LIMIT: Duration = Duration::from_millis(500);
const READER_JOIN_GRACE: Duration = Duration::from_millis(100);

trait EventSource: Send {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    fn read(&mut self) -> io::Result<Event>;
}

struct CrosstermEventSource;

pub(crate) struct KeyInspection {
    pub(crate) raw_event: String,
    pub(crate) matched_action: Option<String>,
}

pub(crate) fn inspect_keypress() -> Result<KeyInspection, TerminalError> {
    eprintln!("Press one key to inspect its terminal event and Proqi action.");
    let guard = RawInputGuard::enter()?;
    let key = loop {
        match event::read()? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                break key;
            }
            Event::FocusGained
            | Event::FocusLost
            | Event::Key(_)
            | Event::Mouse(_)
            | Event::Paste(_)
            | Event::Resize(_, _) => {}
        }
    };
    guard.finish()?;
    Ok(KeyInspection {
        raw_event: format!("{key:?}"),
        matched_action: translate_key(key).map(|action| format!("{action:?}")),
    })
}

struct RawInputGuard {
    active: bool,
}

impl RawInputGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self { active: true })
    }

    fn finish(mut self) -> io::Result<()> {
        disable_raw_mode()?;
        self.active = false;
        Ok(())
    }
}

impl Drop for RawInputGuard {
    fn drop(&mut self) {
        if self.active {
            let _restored = disable_raw_mode();
        }
    }
}

impl EventSource for CrosstermEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }
}

pub(super) enum InputMessage {
    Event { sequence: u64, input: UiInput },
    Failed(InputFailure),
}

pub(super) struct InputLane {
    pub(super) receiver: Receiver<InputMessage>,
    stop: Arc<AtomicBool>,
    latest_sequence: Arc<AtomicU64>,
    handle: Option<JoinHandle<()>>,
}

impl InputLane {
    pub(super) fn spawn() -> Self {
        Self::spawn_with_source(Box::new(CrosstermEventSource))
    }

    fn spawn_with_source(source: Box<dyn EventSource>) -> Self {
        let (sender, receiver) = sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let latest_sequence = Arc::new(AtomicU64::new(0));
        let worker_stop = Arc::clone(&stop);
        let worker_sequence = Arc::clone(&latest_sequence);
        let handle = thread::spawn(move || {
            supervise_input(source, &sender, &worker_stop, &worker_sequence);
        });
        Self {
            receiver,
            stop,
            latest_sequence,
            handle: Some(handle),
        }
    }

    pub(super) fn stop(mut self, deadline: ShutdownDeadline) -> Result<(), TerminalError> {
        self.request_stop();
        join_before(
            self.handle.take(),
            deadline,
            "input lane panicked",
            "input lane did not stop before the shutdown deadline",
        )
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub(super) fn latest_sequence(&self) -> u64 {
        self.latest_sequence.load(Ordering::Acquire)
    }
}

impl Drop for InputLane {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

enum SourceMessage {
    Responsive,
    Event(Event),
    Failed(InputFailure),
}

fn supervise_input(
    mut source: Box<dyn EventSource>,
    sender: &SyncSender<InputMessage>,
    stop: &AtomicBool,
    latest_sequence: &AtomicU64,
) {
    let (source_sender, source_receiver) = sync_channel(64);
    let source_stop = Arc::new(AtomicBool::new(false));
    let reader_stop = Arc::clone(&source_stop);
    let reader = thread::spawn(move || {
        read_source(&mut *source, &source_sender, &reader_stop);
    });
    let mut pending_resize = None;
    let mut last_response = Instant::now();
    while !stop.load(Ordering::Acquire) {
        flush_resize(sender, &mut pending_resize);
        match source_receiver.recv_timeout(MONITOR_INTERVAL) {
            Ok(SourceMessage::Responsive) => last_response = Instant::now(),
            Ok(SourceMessage::Event(event)) => {
                last_response = Instant::now();
                deliver(event, sender, stop, &mut pending_resize, latest_sequence);
            }
            Ok(SourceMessage::Failed(failure)) => {
                let _sent = send_lossless(sender, InputMessage::Failed(failure), stop);
                break;
            }
            Err(RecvTimeoutError::Timeout) if last_response.elapsed() >= SOURCE_STALL_LIMIT => {
                let _sent = send_lossless(
                    sender,
                    InputMessage::Failed(InputFailure::Unresponsive),
                    stop,
                );
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let _sent = send_lossless(
                    sender,
                    InputMessage::Failed(InputFailure::Io(
                        "terminal event reader exited unexpectedly".to_owned(),
                    )),
                    stop,
                );
                break;
            }
        }
    }
    source_stop.store(true, Ordering::Release);
    let _joined = join_before(
        Some(reader),
        ShutdownDeadline::after(READER_JOIN_GRACE),
        "terminal event reader panicked",
        "terminal event reader did not stop",
    );
}

fn read_source(
    source: &mut dyn EventSource,
    sender: &SyncSender<SourceMessage>,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        let message = match source.poll(SOURCE_POLL_INTERVAL) {
            Ok(false) => SourceMessage::Responsive,
            Ok(true) => match source.read() {
                Ok(event) => SourceMessage::Event(event),
                Err(error) => SourceMessage::Failed(classify_input_error(&error)),
            },
            Err(error) => SourceMessage::Failed(classify_input_error(&error)),
        };
        let failed = matches!(message, SourceMessage::Failed(_));
        if !send_source(message, sender, stop) || failed {
            return;
        }
    }
}

fn send_source(
    mut message: SourceMessage,
    sender: &SyncSender<SourceMessage>,
    stop: &AtomicBool,
) -> bool {
    loop {
        match sender.try_send(message) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) if !stop.load(Ordering::Acquire) => {
                message = returned;
                thread::yield_now();
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn classify_input_error(error: &io::Error) -> InputFailure {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        return InputFailure::EndOfFile;
    }
    if matches!(error.raw_os_error(), Some(5 | 6)) {
        return InputFailure::TerminalRevoked;
    }
    InputFailure::Io(error.to_string())
}

fn deliver(
    event: Event,
    sender: &SyncSender<InputMessage>,
    stop: &AtomicBool,
    pending_resize: &mut Option<UiInput>,
    latest_sequence: &AtomicU64,
) {
    let Some(input) = translate(event) else {
        return;
    };
    if matches!(input, UiInput::Resize { .. }) {
        *pending_resize = Some(input);
    } else {
        let sequence = latest_sequence.fetch_add(1, Ordering::AcqRel) + 1;
        let _sent = send_lossless(sender, InputMessage::Event { sequence, input }, stop);
    }
}

fn flush_resize(sender: &SyncSender<InputMessage>, pending: &mut Option<UiInput>) {
    let Some(input) = pending.take() else {
        return;
    };
    match sender.try_send(InputMessage::Event { sequence: 0, input }) {
        Err(TrySendError::Full(InputMessage::Event { input, .. })) => *pending = Some(input),
        Ok(())
        | Err(TrySendError::Disconnected(_) | TrySendError::Full(InputMessage::Failed(_))) => {}
    }
}

fn send_lossless(
    sender: &SyncSender<InputMessage>,
    mut message: InputMessage,
    stop: &AtomicBool,
) -> bool {
    while !stop.load(Ordering::Acquire) {
        match sender.try_send(message) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                message = returned;
                thread::sleep(Duration::from_millis(2));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
    false
}

#[cfg(test)]
#[path = "input/tests.rs"]
mod tests;
