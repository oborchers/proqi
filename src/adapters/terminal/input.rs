//! Crossterm event normalization and lossless input delivery.

use std::{
    fmt, io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

use crate::ports::editor::CursorMovement;
use crate::ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey};

use super::{
    TerminalError,
    supervisor::{ShutdownDeadline, join_before},
};

#[derive(Debug, Eq, PartialEq)]
pub(super) enum InputFailure {
    EndOfFile,
    TerminalRevoked,
    Io(String),
}

impl fmt::Display for InputFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfFile => formatter.write_str("terminal input reached end of file"),
            Self::TerminalRevoked => formatter.write_str("terminal input was revoked"),
            Self::Io(message) => write!(formatter, "terminal input failed: {message}"),
        }
    }
}

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

    fn spawn_with_source(mut source: Box<dyn EventSource>) -> Self {
        let (sender, receiver) = sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let latest_sequence = Arc::new(AtomicU64::new(0));
        let worker_stop = Arc::clone(&stop);
        let worker_sequence = Arc::clone(&latest_sequence);
        let handle = thread::spawn(move || {
            input_loop(&mut *source, &sender, &worker_stop, &worker_sequence);
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

fn input_loop(
    source: &mut dyn EventSource,
    sender: &SyncSender<InputMessage>,
    stop: &AtomicBool,
    latest_sequence: &AtomicU64,
) {
    let mut pending_resize = None;
    while !stop.load(Ordering::Acquire) {
        flush_resize(sender, &mut pending_resize);
        match source.poll(Duration::from_millis(40)) {
            Ok(false) => {}
            Ok(true) => match source.read() {
                Ok(event) => deliver(event, sender, stop, &mut pending_resize, latest_sequence),
                Err(error) => {
                    let _sent = send_lossless(
                        sender,
                        InputMessage::Failed(classify_input_error(&error)),
                        stop,
                    );
                    return;
                }
            },
            Err(error) => {
                let _sent = send_lossless(
                    sender,
                    InputMessage::Failed(classify_input_error(&error)),
                    stop,
                );
                return;
            }
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

fn translate(event: Event) -> Option<UiInput> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            translate_key(key).map(UiInput::Key)
        }
        Event::Paste(content) => Some(
            super::path_import::annotate_existing_files(&content)
                .map_or_else(|| UiInput::Paste(content), UiInput::PasteAnnotated),
        ),
        Event::Resize(width, height) => Some(UiInput::Resize { width, height }),
        Event::Mouse(mouse) => translate_mouse(mouse).map(UiInput::Pointer),
        Event::FocusGained => Some(UiInput::HostFocusGained),
        Event::FocusLost | Event::Key(_) => None,
    }
}

fn translate_mouse(mouse: MouseEvent) -> Option<PointerInput> {
    let kind = match mouse.kind {
        MouseEventKind::Down(button) => PointerKind::Down(pointer_button(button)),
        MouseEventKind::Up(button) => PointerKind::Up(pointer_button(button)),
        MouseEventKind::Drag(button) => PointerKind::Drag(pointer_button(button)),
        MouseEventKind::Moved => PointerKind::Move,
        MouseEventKind::ScrollUp => PointerKind::ScrollUp,
        MouseEventKind::ScrollDown => PointerKind::ScrollDown,
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => return None,
    };
    Some(PointerInput {
        column: mouse.column,
        row: mouse.row,
        kind,
    })
}

const fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Right => PointerButton::Right,
    }
}

fn translate_key(key: KeyEvent) -> Option<UiKey> {
    let primary = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META);
    if primary {
        let command = match key.code {
            KeyCode::Char('a') => Some(UiKey::SelectAll),
            KeyCode::Char('c') => Some(UiKey::Copy),
            KeyCode::Char('x') => Some(UiKey::Cut),
            KeyCode::Char('v') => Some(UiKey::PasteClipboard),
            KeyCode::Char('q') => Some(UiKey::Quit),
            KeyCode::Char('u') => Some(UiKey::DeleteLine),
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::SHIFT) => Some(UiKey::Redo),
            KeyCode::Char('z') => Some(UiKey::Undo),
            KeyCode::Char('y') => Some(UiKey::Redo),
            _ => None,
        };
        if command.is_some() || matches!(key.code, KeyCode::Char(_)) {
            return command;
        }
    }
    let extend_selection = key.modifiers.contains(KeyModifiers::SHIFT);
    let word = key
        .modifiers
        .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL);
    let document = key
        .modifiers
        .intersects(KeyModifiers::SUPER | KeyModifiers::META);
    match key.code {
        KeyCode::Char(character) => Some(UiKey::Character(character)),
        KeyCode::Enter => Some(UiKey::Enter),
        KeyCode::Esc => Some(UiKey::Escape),
        KeyCode::Backspace => Some(UiKey::Backspace),
        KeyCode::Delete => Some(UiKey::Delete),
        KeyCode::Up => Some(move_key(
            if document {
                CursorMovement::DocumentStart
            } else {
                CursorMovement::VisualUp
            },
            extend_selection,
        )),
        KeyCode::Down => Some(move_key(
            if document {
                CursorMovement::DocumentEnd
            } else {
                CursorMovement::VisualDown
            },
            extend_selection,
        )),
        KeyCode::Left => Some(move_key(
            if word {
                CursorMovement::WordBack
            } else {
                CursorMovement::GraphemeBack
            },
            extend_selection,
        )),
        KeyCode::Right => Some(move_key(
            if word {
                CursorMovement::WordForward
            } else {
                CursorMovement::GraphemeForward
            },
            extend_selection,
        )),
        KeyCode::Home => Some(move_key(CursorMovement::LineStart, extend_selection)),
        KeyCode::End => Some(move_key(CursorMovement::LineEnd, extend_selection)),
        _ => None,
    }
}

const fn move_key(movement: CursorMovement, extend_selection: bool) -> UiKey {
    UiKey::Move {
        movement,
        extend_selection,
    }
}

#[cfg(test)]
#[path = "input/tests.rs"]
mod tests;
