//! Crossterm event normalization and lossless input delivery.

use std::{
    fmt, fs, io,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossterm::event::{self, Event};

use crate::ui::UiInput;

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, join_before},
};

use translation::translate;

mod inspection;
mod observation;
use observation::{LeaseDecision, ReaderProgress, SourceLease};
mod translation;
pub(crate) use inspection::inspect_keypress;

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

impl EventSource for CrosstermEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }
}

struct StallInjectingEventSource {
    inner: CrosstermEventSource,
    trigger: Option<PathBuf>,
    stall_on_first_poll: bool,
}

impl StallInjectingEventSource {
    fn new(runtime_directory: Option<&Path>) -> Self {
        let trigger = std::env::var_os("PROQI_TEST_INPUT_STALL").and_then(|_| {
            std::env::var_os("PROQI_TEST_INPUT_STALL_TRIGGER")
                .map(PathBuf::from)
                .or_else(|| runtime_directory.map(|root| root.join("input-stall-trigger")))
        });
        Self {
            inner: CrosstermEventSource,
            trigger,
            stall_on_first_poll: std::env::var_os("PROQI_TEST_INPUT_STALL_PROBATION").is_some()
                && std::env::var_os("PROQI_INPUT_RECOVERY_SESSION").is_some(),
        }
    }
}

impl EventSource for StallInjectingEventSource {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        if std::mem::take(&mut self.stall_on_first_poll)
            || self
                .trigger
                .as_ref()
                .is_some_and(|path| fs::remove_file(path).is_ok())
        {
            loop {
                thread::park();
            }
        }
        self.inner.poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        self.inner.read()
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
    completed_polls: Arc<AtomicU64>,
    test_acceptance_path: Option<PathBuf>,
    handle: Option<JoinHandle<()>>,
}

impl InputLane {
    pub(super) fn spawn() -> Self {
        Self::spawn_source(Box::new(StallInjectingEventSource::new(None)), None)
    }

    pub(super) fn spawn_with_test_acceptance(runtime_directory: &Path) -> Self {
        Self::spawn_source(
            Box::new(StallInjectingEventSource::new(Some(runtime_directory))),
            Some(runtime_directory),
        )
    }

    #[cfg(test)]
    fn spawn_with_source(source: Box<dyn EventSource>) -> Self {
        Self::spawn_source(source, None)
    }

    fn spawn_source(source: Box<dyn EventSource>, runtime_directory: Option<&Path>) -> Self {
        let (sender, receiver) = sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let latest_sequence = Arc::new(AtomicU64::new(0));
        let completed_polls = Arc::new(AtomicU64::new(0));
        let worker_stop = Arc::clone(&stop);
        let worker_sequence = Arc::clone(&latest_sequence);
        let worker_polls = Arc::clone(&completed_polls);
        let handle = thread::spawn(move || {
            supervise_input(
                source,
                &sender,
                &worker_stop,
                &worker_sequence,
                &worker_polls,
            );
        });
        Self {
            receiver,
            stop,
            latest_sequence,
            completed_polls,
            test_acceptance_path: std::env::var_os("PROQI_TEST_INPUT_ACCEPTANCE")
                .and_then(|_| runtime_directory.map(|root| root.join("input-accepted"))),
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

    pub(super) fn completed_polls(&self) -> u64 {
        self.completed_polls.load(Ordering::Acquire)
    }

    pub(super) fn record_test_acceptance(&self, sequence: u64, mode: &str) {
        let Some(path) = &self.test_acceptance_path else {
            return;
        };
        wait_for_test_input_release();
        let result = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut output| writeln!(output, "{sequence} mode={mode}"));
        debug_assert!(
            result.is_ok(),
            "test input-acceptance probe must be writable"
        );
    }
}

fn wait_for_test_input_release() {
    if std::env::var_os("PROQI_TEST_INPUT_STALL").is_none() {
        return;
    }
    let Some(arm) = std::env::var_os("PROQI_TEST_INPUT_BARRIER_ARM").map(PathBuf::from) else {
        return;
    };
    if fs::remove_file(arm).is_err() {
        return;
    }
    let Some(pending) = std::env::var_os("PROQI_TEST_INPUT_BARRIER_PENDING").map(PathBuf::from)
    else {
        return;
    };
    let Some(release) = std::env::var_os("PROQI_TEST_INPUT_BARRIER_RELEASE").map(PathBuf::from)
    else {
        return;
    };
    let _written = fs::write(pending, b"pending");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !release.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    debug_assert!(release.exists(), "test input barrier was not released");
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
    completed_polls: &AtomicU64,
) {
    let (source_sender, source_receiver) = sync_channel(64);
    let source_stop = Arc::new(AtomicBool::new(false));
    let reader_stop = Arc::clone(&source_stop);
    let progress = Arc::new(ReaderProgress::new(Instant::now()));
    let reader_progress = Arc::clone(&progress);
    let reader = thread::spawn(move || {
        read_source(&mut *source, &source_sender, &reader_stop, &reader_progress);
    });
    let mut pending_resize = None;
    let mut lease = SourceLease::new(Instant::now());
    while !stop.load(Ordering::Acquire) {
        flush_resize(sender, &mut pending_resize);
        match source_receiver.recv_timeout(MONITOR_INTERVAL) {
            Ok(SourceMessage::Responsive) => {
                completed_polls.fetch_add(1, Ordering::AcqRel);
                let _decision = lease.observe(Instant::now(), true);
            }
            Ok(SourceMessage::Event(event)) => {
                completed_polls.fetch_add(1, Ordering::AcqRel);
                let _decision = lease.observe(Instant::now(), true);
                deliver(event, sender, stop, &mut pending_resize, latest_sequence);
            }
            Ok(SourceMessage::Failed(failure)) => {
                let _sent = send_lossless(sender, InputMessage::Failed(failure), stop);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                let evidence = lease.evidence(now, &progress);
                match lease.observe(now, false) {
                    LeaseDecision::Continue => {}
                    LeaseDecision::ResetAfterSupervisorGap { .. } => {
                        observation::record_supervisor_gap(evidence);
                    }
                    LeaseDecision::Unresponsive => {
                        observation::record_stall(evidence);
                        let _sent = send_lossless(
                            sender,
                            InputMessage::Failed(InputFailure::Unresponsive),
                            stop,
                        );
                        break;
                    }
                }
            }
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
    progress: &ReaderProgress,
) {
    use crate::adapters::diagnostics::InputReaderStage::{Delivery, Poll, Read, Stopped};
    while !stop.load(Ordering::Acquire) {
        progress.enter(Poll, Instant::now());
        let polled = source.poll(SOURCE_POLL_INTERVAL);
        progress.complete(Instant::now());
        let message = match polled {
            Ok(false) => SourceMessage::Responsive,
            Ok(true) => {
                progress.enter(Read, Instant::now());
                let read = source.read();
                progress.complete(Instant::now());
                match read {
                    Ok(event) => SourceMessage::Event(event),
                    Err(error) => SourceMessage::Failed(classify_input_error(&error)),
                }
            }
            Err(error) => SourceMessage::Failed(classify_input_error(&error)),
        };
        let failed = matches!(message, SourceMessage::Failed(_));
        progress.enter(Delivery, Instant::now());
        let delivered = send_source(message, sender, stop);
        if delivered {
            progress.complete(Instant::now());
        }
        if !delivered || failed {
            break;
        }
    }
    progress.enter(Stopped, Instant::now());
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
