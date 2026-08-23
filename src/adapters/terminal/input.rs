//! Crossterm event normalization and lossless input delivery.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::ui::{UiInput, UiKey};

pub(super) enum InputMessage {
    Event(UiInput),
    Failed(String),
}

pub(super) struct InputLane {
    pub(super) receiver: Receiver<InputMessage>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl InputLane {
    pub(super) fn spawn() -> Self {
        let (sender, receiver) = sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let handle = thread::spawn(move || input_loop(&sender, &worker_stop));
        Self {
            receiver,
            stop,
            handle: Some(handle),
        }
    }

    pub(super) fn stop(mut self) -> thread::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.handle.take().map_or(Ok(()), JoinHandle::join)
    }
}

fn input_loop(sender: &SyncSender<InputMessage>, stop: &AtomicBool) {
    let mut pending_resize = None;
    while !stop.load(Ordering::Acquire) {
        flush_resize(sender, &mut pending_resize);
        match event::poll(Duration::from_millis(40)) {
            Ok(false) => {}
            Ok(true) => match event::read() {
                Ok(event) => deliver(event, sender, stop, &mut pending_resize),
                Err(error) => {
                    let _sent =
                        send_lossless(sender, InputMessage::Failed(error.to_string()), stop);
                    return;
                }
            },
            Err(error) => {
                let _sent = send_lossless(sender, InputMessage::Failed(error.to_string()), stop);
                return;
            }
        }
    }
}

fn deliver(
    event: Event,
    sender: &SyncSender<InputMessage>,
    stop: &AtomicBool,
    pending_resize: &mut Option<UiInput>,
) {
    let Some(input) = translate(event) else {
        return;
    };
    if matches!(input, UiInput::Resize { .. }) {
        *pending_resize = Some(input);
    } else {
        let _sent = send_lossless(sender, InputMessage::Event(input), stop);
    }
}

fn flush_resize(sender: &SyncSender<InputMessage>, pending: &mut Option<UiInput>) {
    let Some(input) = pending.take() else {
        return;
    };
    match sender.try_send(InputMessage::Event(input)) {
        Err(TrySendError::Full(InputMessage::Event(input))) => *pending = Some(input),
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
        Event::Key(key) if key.kind == KeyEventKind::Press => translate_key(key).map(UiInput::Key),
        Event::Paste(content) => Some(UiInput::Paste(content)),
        Event::Resize(width, height) => Some(UiInput::Resize { width, height }),
        Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Key(_) => None,
    }
}

fn translate_key(key: KeyEvent) -> Option<UiKey> {
    let command = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER);
    if command {
        return match key.code {
            KeyCode::Char('a') => Some(UiKey::SelectAll),
            KeyCode::Char('c') => Some(UiKey::Quit),
            KeyCode::Char('u') => Some(UiKey::DeleteLine),
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::SHIFT) => Some(UiKey::Redo),
            KeyCode::Char('z') => Some(UiKey::Undo),
            KeyCode::Char('y') => Some(UiKey::Redo),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char(character) => Some(UiKey::Character(character)),
        KeyCode::Enter => Some(UiKey::Enter),
        KeyCode::Esc => Some(UiKey::Escape),
        KeyCode::Backspace => Some(UiKey::Backspace),
        KeyCode::Delete => Some(UiKey::Delete),
        KeyCode::Up => Some(UiKey::Up),
        KeyCode::Down => Some(UiKey::Down),
        KeyCode::Left => Some(UiKey::Left),
        KeyCode::Right => Some(UiKey::Right),
        KeyCode::Home => Some(UiKey::Home),
        KeyCode::End => Some(UiKey::End),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use crate::ui::{UiInput, UiKey};

    use super::translate;

    #[test]
    fn command_and_meta_shortcuts_share_semantics() {
        for modifier in [KeyModifiers::CONTROL, KeyModifiers::SUPER] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), modifier));
            assert_eq!(translate(event), Some(UiInput::Key(UiKey::SelectAll)));
        }
    }

    #[test]
    fn bracketed_paste_remains_one_exact_input() {
        let content = "Grüße 👩‍💻\n第二行\n".to_owned();
        assert_eq!(
            translate(Event::Paste(content.clone())),
            Some(UiInput::Paste(content))
        );
    }
}
