use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    adapters::terminal::supervisor::ShutdownDeadline,
    ports::editor::CursorMovement,
    ui::{UiInput, UiKey},
};

use super::{EventSource, InputFailure, InputLane, InputMessage, translate};

#[path = "tests/pointer.rs"]
mod pointer;

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
fn primary_shift_s_remains_an_unassigned_board_chord() {
    for character in ['s', 'S'] {
        let event = Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        ));
        assert_eq!(
            translate(event),
            Some(UiInput::Key(UiKey::PrimaryCharacter(character)))
        );
    }
}

#[test]
fn logical_line_and_sentence_deletion_keep_distinct_primary_chords() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Char('u'), modifier))),
            Some(UiInput::Key(UiKey::DeleteLogicalLine))
        );
        for character in ['u', 'U'] {
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    modifier | KeyModifiers::SHIFT,
                ))),
                Some(UiInput::Key(UiKey::PrimaryCharacter(character)))
            );
        }
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Char('U'), modifier))),
            Some(UiInput::Key(UiKey::PrimaryCharacter('U')))
        );
    }

    assert_eq!(
        translate(Event::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::ALT,
        ))),
        Some(UiInput::Key(UiKey::Character('u')))
    );
}

#[test]
fn primary_shift_z_is_redo_for_both_terminal_case_encodings() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        for (character, modifiers) in [
            ('z', modifier | KeyModifiers::SHIFT),
            ('Z', modifier | KeyModifiers::SHIFT),
            ('Z', modifier),
        ] {
            assert_eq!(
                translate(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    modifiers,
                ))),
                Some(UiInput::Key(UiKey::Redo))
            );
        }
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
    for event in [
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT),
    ] {
        assert_eq!(
            translate(Event::Key(event)),
            Some(UiInput::Key(UiKey::BackTab))
        );
    }
}

#[test]
fn physical_delete_and_backspace_remain_distinct_terminal_keys() {
    for (code, expected) in [
        (KeyCode::Delete, UiKey::Delete),
        (KeyCode::Backspace, UiKey::Backspace),
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))),
            Some(UiInput::Key(expected))
        );
    }
}

#[test]
fn every_modified_physical_delete_remains_distinct_from_board_delete() {
    for modifiers in [
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        KeyModifiers::SUPER | KeyModifiers::SHIFT,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Delete, modifiers))),
            Some(UiInput::Key(UiKey::ModifiedDelete)),
            "modifiers: {modifiers:?}"
        );
    }
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

    let repeated_range = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::SHIFT,
        KeyEventKind::Repeat,
    ));
    assert_eq!(
        translate(repeated_range),
        Some(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: true,
        }))
    );

    let repeated_reorder = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Down,
        KeyModifiers::SHIFT | KeyModifiers::SUPER,
        KeyEventKind::Repeat,
    ));
    assert_eq!(
        translate(repeated_reorder),
        Some(UiInput::Key(UiKey::PrimaryShiftMove {
            movement: CursorMovement::DocumentEnd,
        }))
    );
}

#[test]
fn primary_shift_arrow_and_character_chords_remain_board_semantics() {
    let arrow = Event::Key(KeyEvent::new(
        KeyCode::Up,
        KeyModifiers::SHIFT | KeyModifiers::CONTROL,
    ));
    assert_eq!(
        translate(arrow),
        Some(UiInput::Key(UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualUp,
        }))
    );

    let character = Event::Key(KeyEvent::new(
        KeyCode::Char('K'),
        KeyModifiers::SHIFT | KeyModifiers::SUPER,
    ));
    assert_eq!(
        translate(character),
        Some(UiInput::Key(UiKey::PrimaryCharacter('K')))
    );

    let alternate_report = Event::Key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SUPER));
    assert_eq!(
        translate(alternate_report),
        Some(UiInput::Key(UiKey::PrimaryCharacter('K')))
    );
}

#[test]
fn unknown_primary_character_shortcuts_never_insert_text() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('b'), modifier));
        assert_eq!(
            translate(event),
            Some(UiInput::Key(UiKey::PrimaryCharacter('b')))
        );
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
fn unshifted_alt_and_platform_primary_arrows_preserve_both_mode_intentions() {
    assert_eq!(
        translate(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT,))),
        Some(UiInput::Key(UiKey::EditNavigation {
            editor_movement: CursorMovement::VisualJumpUp,
            board_movement: CursorMovement::VisualUp,
        }))
    );
    assert_eq!(
        translate(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT,))),
        Some(UiInput::Key(UiKey::EditNavigation {
            editor_movement: CursorMovement::VisualJumpDown,
            board_movement: CursorMovement::VisualDown,
        }))
    );

    let modifier = if cfg!(target_os = "macos") {
        KeyModifiers::SUPER
    } else {
        KeyModifiers::CONTROL
    };
    let board_up = if cfg!(target_os = "macos") {
        CursorMovement::DocumentStart
    } else {
        CursorMovement::VisualUp
    };
    let board_down = if cfg!(target_os = "macos") {
        CursorMovement::DocumentEnd
    } else {
        CursorMovement::VisualDown
    };
    assert_eq!(
        translate(Event::Key(KeyEvent::new(KeyCode::Up, modifier))),
        Some(UiInput::Key(UiKey::EditNavigation {
            editor_movement: CursorMovement::DocumentStart,
            board_movement: board_up,
        }))
    );
    assert_eq!(
        translate(Event::Key(KeyEvent::new(KeyCode::Down, modifier))),
        Some(UiInput::Key(UiKey::EditNavigation {
            editor_movement: CursorMovement::DocumentEnd,
            board_movement: board_down,
        }))
    );
}

#[test]
fn shifted_alt_arrows_keep_the_existing_one_row_selection_intention() {
    assert_eq!(
        translate(Event::Key(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        ))),
        Some(UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
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
fn primary_enter_chords_are_distinct_from_plain_multiline_enter() {
    for modifier in [
        KeyModifiers::CONTROL,
        KeyModifiers::SUPER,
        KeyModifiers::META,
    ] {
        assert_eq!(
            translate(Event::Key(KeyEvent::new(KeyCode::Enter, modifier))),
            Some(UiInput::Key(UiKey::Submit))
        );
        assert_eq!(
            translate(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                modifier | KeyModifiers::SHIFT,
            ))),
            Some(UiInput::Key(UiKey::SubmitKeep))
        );
    }
}

#[test]
fn host_focus_events_are_distinct_normalized_passive_signals() {
    assert_eq!(
        translate(Event::FocusGained),
        Some(UiInput::HostFocusGained)
    );
    assert_eq!(translate(Event::FocusLost), Some(UiInput::HostFocusLost));
}
