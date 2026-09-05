//! Crossterm event normalization and lossless input delivery.

use std::{
    fmt,
    io::{self, stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{
    event::{self, Event, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
};

use crate::ui::UiInput;

use super::{
    TerminalError,
    control::{compatible_keyboard_flags, reset_keyboard_reporting},
    supervisor::{ShutdownDeadline, join_before},
};

use translation::translate;

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

pub(crate) fn inspect_keypress(
    shortcut_registry: &crate::ui::ShortcutRegistry,
) -> Result<KeyInspection, TerminalError> {
    let guard = RawInputGuard::enter()?;
    eprintln!("Press one key to inspect its terminal event and Proqi action.");
    let (raw_event, stroke) = loop {
        let event = event::read()?;
        let raw_event = match &event {
            Event::Key(key) => format!("{key:?}"),
            _ => format!("{event:?}"),
        };
        if let Some(UiInput::KeyStroke(stroke)) = translate(event) {
            break (raw_event, stroke);
        }
    };
    guard.finish()?;
    let contexts = crate::ui::ShortcutContextStack::new([crate::ui::ShortcutContext::Board]);
    Ok(KeyInspection {
        raw_event,
        matched_action: Some(
            shortcut_registry
                .diagnostics_id(&contexts, stroke)
                .to_owned(),
        ),
    })
}

struct RawInputGuard {
    active: bool,
    keyboard_active: bool,
}

impl RawInputGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let keyboard_active = execute!(
            stdout(),
            PushKeyboardEnhancementFlags(compatible_keyboard_flags())
        )
        .is_ok();
        Ok(Self {
            active: true,
            keyboard_active,
        })
    }

    fn finish(mut self) -> io::Result<()> {
        self.restore()
    }

    fn restore(&mut self) -> io::Result<()> {
        let keyboard = if self.keyboard_active {
            execute!(stdout(), PopKeyboardEnhancementFlags)
        } else {
            Ok(())
        };
        self.keyboard_active = false;
        let reset = reset_keyboard_reporting();
        let raw = if self.active {
            disable_raw_mode()
        } else {
            Ok(())
        };
        self.active = false;
        keyboard.and(reset).and(raw)
    }
}

impl Drop for RawInputGuard {
    fn drop(&mut self) {
        let _restored = self.restore();
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

#[derive(Debug, Eq, PartialEq)]
enum LeaseDecision {
    Continue,
    ResetAfterSupervisorGap { gap: Duration },
    Unresponsive,
}

struct SourceLease {
    last_response: Instant,
    last_observation: Instant,
}

impl SourceLease {
    fn new(now: Instant) -> Self {
        Self {
            last_response: now,
            last_observation: now,
        }
    }

    fn observe(&mut self, now: Instant, reader_responded: bool) -> LeaseDecision {
        let supervisor_gap = now.saturating_duration_since(self.last_observation);
        self.last_observation = now;
        if reader_responded {
            self.last_response = now;
            return LeaseDecision::Continue;
        }
        if supervisor_gap >= SOURCE_STALL_LIMIT {
            self.last_response = now;
            return LeaseDecision::ResetAfterSupervisorGap {
                gap: supervisor_gap,
            };
        }
        if now.saturating_duration_since(self.last_response) >= SOURCE_STALL_LIMIT {
            LeaseDecision::Unresponsive
        } else {
            LeaseDecision::Continue
        }
    }
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
    let mut lease = SourceLease::new(Instant::now());
    while !stop.load(Ordering::Acquire) {
        flush_resize(sender, &mut pending_resize);
        match source_receiver.recv_timeout(MONITOR_INTERVAL) {
            Ok(SourceMessage::Responsive) => {
                let _decision = lease.observe(Instant::now(), true);
            }
            Ok(SourceMessage::Event(event)) => {
                let _decision = lease.observe(Instant::now(), true);
                deliver(event, sender, stop, &mut pending_resize, latest_sequence);
            }
            Ok(SourceMessage::Failed(failure)) => {
                let _sent = send_lossless(sender, InputMessage::Failed(failure), stop);
                break;
            }
            Err(RecvTimeoutError::Timeout) => match lease.observe(Instant::now(), false) {
                LeaseDecision::Continue => {}
                LeaseDecision::ResetAfterSupervisorGap { gap } => {
                    crate::adapters::diagnostics::record_input_lease_reset(
                        u64::try_from(gap.as_millis()).unwrap_or(u64::MAX),
                    );
                }
                LeaseDecision::Unresponsive => {
                    let _sent = send_lossless(
                        sender,
                        InputMessage::Failed(InputFailure::Unresponsive),
                        stop,
                    );
                    break;
                }
            },
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
#[cfg(test)]
#[path = "input/translation_tests.rs"]
mod translation_tests;
