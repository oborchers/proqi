use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::{
    adapters::terminal::supervisor::ShutdownDeadline,
    ports::editor::CursorMovement,
    ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};

use super::{EventSource, InputFailure, InputLane, InputMessage, translate};

struct FakeSource {
    polls: VecDeque<io::Result<bool>>,
    reads: VecDeque<io::Result<Event>>,
    delay: Duration,
    entered: Option<Arc<AtomicBool>>,
}

impl EventSource for FakeSource {
    fn poll(&mut self, _timeout: Duration) -> io::Result<bool> {
        if let Some(entered) = &self.entered {
            entered.store(true, Ordering::Release);
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        self.polls.pop_front().unwrap_or(Ok(false))
    }

    fn read(&mut self) -> io::Result<Event> {
        self.reads
            .pop_front()
            .unwrap_or_else(|| Err(io::Error::other("missing fake event")))
    }
}

fn source_with_poll(result: io::Result<bool>) -> Box<dyn EventSource> {
    Box::new(FakeSource {
        polls: VecDeque::from([result]),
        reads: VecDeque::new(),
        delay: Duration::ZERO,
        entered: None,
    })
}

#[test]
fn eof_and_revoked_terminal_errors_remain_typed() {
    for (error, expected) in [
        (
            io::Error::new(io::ErrorKind::UnexpectedEof, "closed"),
            InputFailure::EndOfFile,
        ),
        (
            io::Error::from_raw_os_error(5),
            InputFailure::TerminalRevoked,
        ),
    ] {
        let lane = InputLane::spawn_with_source(source_with_poll(Err(error)));
        let InputMessage::Failed(actual) = lane
            .receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("typed input failure")
        else {
            panic!("expected a failure message");
        };
        assert_eq!(actual, expected);
        lane.stop(ShutdownDeadline::after(Duration::from_secs(1)))
            .expect("failed input lane stops");
    }
}

#[test]
fn nonresponsive_source_cannot_make_stop_wait_without_bound() {
    let entered = Arc::new(AtomicBool::new(false));
    let lane = InputLane::spawn_with_source(Box::new(FakeSource {
        polls: VecDeque::new(),
        reads: VecDeque::new(),
        delay: Duration::from_millis(200),
        entered: Some(Arc::clone(&entered)),
    }));
    while !entered.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    let started = Instant::now();
    let result = lane.stop(ShutdownDeadline::after(Duration::from_millis(10)));
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn stalled_registry_event_source_becomes_a_typed_failure() {
    let lane = InputLane::spawn_with_source(Box::new(FakeSource {
        polls: VecDeque::new(),
        reads: VecDeque::new(),
        delay: Duration::from_secs(2),
        entered: None,
    }));
    let InputMessage::Failed(failure) = lane
        .receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("stalled source failure")
    else {
        panic!("expected a failure message");
    };
    assert_eq!(failure, InputFailure::Unresponsive);
    lane.stop(ShutdownDeadline::after(Duration::from_secs(1)))
        .expect("input supervisor stops without the stalled reader");
}

#[test]
fn command_and_meta_shortcuts_share_semantics() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), modifier));
        assert_eq!(translate(event), Some(UiInput::Key(UiKey::SelectAll)));
    }
}

#[test]
fn primary_clipboard_shortcuts_do_not_reuse_quit() {
    for (character, expected) in [
        ('c', UiKey::Copy),
        ('x', UiKey::Cut),
        ('v', UiKey::PasteClipboard),
        ('d', UiKey::Duplicate),
        ('q', UiKey::Quit),
    ] {
        let event = Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(translate(event), Some(UiInput::Key(expected)));
    }
}

#[test]
fn invocation_picker_keys_are_normalized_without_literal_editor_input() {
    for (code, expected) in [
        (KeyCode::Char('p'), UiKey::PickerPrevious),
        (KeyCode::Char('n'), UiKey::PickerNext),
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL))),
            Some(UiInput::Key(expected))
        );
    }
    assert_eq!(
        translate(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))),
        Some(UiInput::Key(UiKey::Tab))
    );
}

#[test]
fn release_is_ignored_and_repeat_preserves_auto_repeat() {
    let release = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert_eq!(translate(release), None);
    let repeat = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    ));
    assert_eq!(translate(repeat), Some(UiInput::Key(UiKey::Character('n'))));
}

#[test]
fn unknown_primary_character_shortcuts_never_insert_text() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('b'), modifier));
        assert_eq!(translate(event), None);
    }
}

#[test]
fn shift_and_word_navigation_remain_semantic() {
    let select = Event::Key(KeyEvent::new(
        KeyCode::Left,
        KeyModifiers::SHIFT | KeyModifiers::ALT,
    ));
    assert_eq!(
        translate(select),
        Some(UiInput::Key(UiKey::Move {
            movement: CursorMovement::WordBack,
            extend_selection: true,
        }))
    );
}

#[test]
fn bracketed_paste_remains_one_exact_input() {
    let content = "Grüße 👩‍💻\n第二行\n".to_owned();
    assert_eq!(
        translate(Event::Paste(content.clone())),
        Some(UiInput::Paste(content))
    );
}

#[test]
fn enter_is_normalized_independently_from_exact_bracketed_paste() {
    assert_eq!(
        translate(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))),
        Some(UiInput::Key(UiKey::Enter))
    );
    let list = "- pasted".to_owned();
    assert_eq!(
        translate(Event::Paste(list.clone())),
        Some(UiInput::Paste(list))
    );
}

#[test]
fn host_focus_is_a_semantic_refresh_signal() {
    assert_eq!(
        translate(Event::FocusGained),
        Some(UiInput::HostFocusGained)
    );
    assert_eq!(translate(Event::FocusLost), None);
}

#[test]
fn mouse_coordinates_are_normalized_without_terminal_types() {
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 7,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        translate(event),
        Some(UiInput::Pointer(PointerInput {
            column: 7,
            row: 3,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }))
    );
}

#[test]
fn shifted_mouse_input_preserves_selection_extension_intent() {
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: 2,
        modifiers: KeyModifiers::SHIFT,
    });
    assert_eq!(
        translate(event),
        Some(UiInput::Pointer(PointerInput {
            column: 4,
            row: 2,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: true,
        }))
    );
}
