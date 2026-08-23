//! Deterministic complete-board rendering and interaction contracts.

use std::path::PathBuf;

use proqi::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, ClipboardIntent, Effect, FailureCode},
    domain::{OperationSequence, Session, SessionBoard, Timestamp},
    ports::environment::IdGenerator,
    ui::{
        BoardApp, PointerButton, PointerInput, PointerKind, Theme, ThemePreference, UiInput, UiKey,
        UiSettings, render,
    },
};
use ratatui_core::{
    backend::{Backend, TestBackend},
    buffer::Buffer,
    layout::Rect,
    terminal::Terminal,
};

struct Fixture {
    app: BoardApp,
    ids: FakeIdGenerator,
    clock: FakeClock,
}

impl Fixture {
    fn new() -> Self {
        Self::with_settings(UiSettings::default())
    }

    fn with_settings(settings: UiSettings) -> Self {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            PathBuf::from("/tmp/proqi-ui-contract"),
            Timestamp::from_millis(10),
        )
        .expect("session");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        Self {
            app: BoardApp::with_settings(AppState::new(board), settings),
            ids,
            clock: FakeClock::new(Timestamp::from_millis(20)),
        }
    }

    fn input(&mut self, input: UiInput) {
        let _effects = self.app.handle(input, &mut self.ids, &self.clock);
    }

    fn effects(&mut self, input: UiInput) -> Vec<Effect> {
        self.app.handle(input, &mut self.ids, &self.clock)
    }

    fn pointer(&mut self, column: u16, row: u16, kind: PointerKind) {
        self.input(UiInput::Pointer(PointerInput { column, row, kind }));
    }

    fn paste(&mut self, content: &str) -> OperationSequence {
        let effects = self.app.handle(
            UiInput::Paste(content.to_owned()),
            &mut self.ids,
            &self.clock,
        );
        effects
            .first()
            .and_then(proqi::application::Effect::persistence_batch)
            .and_then(|batch| batch.sequence())
            .expect("persistence sequence")
    }
}

fn draw(fixture: &mut Fixture, width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = fixture.app.prepare_frame(frame.area());
            render(
                frame,
                &fixture.app,
                &layout,
                &Theme::resolve(ThemePreference::Auto, true),
            );
        })
        .expect("draw");
    terminal
}

fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn empty_board_and_help_have_reviewable_complete_buffers() {
    let mut fixture = Fixture::new();
    let terminal = draw(&mut fixture, 40, 8);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("+  create a thought with n or paste"));
    assert!(rendered.contains("board  saved"));

    fixture.input(UiInput::Key(UiKey::Character('?')));
    let terminal = draw(&mut fixture, 40, 8);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("proqi help"));
    assert!(rendered.contains("J/K move"));
}

#[test]
fn multiline_unicode_is_rendered_as_lines_and_cursor_uses_cell_width() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("A界\nGrüße 👩‍💻\n第二行");
    let mut terminal = draw(&mut fixture, 40, 10);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("A界"));
    assert!(rendered.contains("Grüße 👩‍💻"));
    assert!(rendered.contains('第'));
    assert!(rendered.contains('二'));
    assert!(rendered.contains('行'));
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor position");
    assert_eq!(cursor.y, 2);
    assert_eq!(cursor.x, 8);

    fixture.app.acknowledge_persistence(sequence, true);
    let terminal = draw(&mut fixture, 40, 10);
    assert!(text(terminal.backend().buffer()).contains("edit  saved"));
}

#[test]
fn repeated_resize_preserves_content_and_logical_cursor() {
    let mut fixture = Fixture::new();
    fixture.paste("one 👩‍💻 two combining e\u{301} three 第二行 four five six");
    let original = fixture.app.editor_snapshot().expect("editor snapshot");

    for (width, height) in [(12, 4), (80, 5), (20, 12), (8, 3), (40, 8)] {
        let terminal = draw(&mut fixture, width, height);
        assert_eq!(terminal.backend().buffer().area.width, width);
        assert_eq!(terminal.backend().buffer().area.height, height);
    }

    let resized = fixture.app.editor_snapshot().expect("editor snapshot");
    assert_eq!(resized.content, original.content);
    assert_eq!(resized.cursor, original.cursor);
    assert!(resized.scroll_row < resized.visual_lines.len());
}

#[test]
fn pending_and_failed_durability_are_visibly_distinct() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("important unsaved text");
    let pending = draw(&mut fixture, 50, 6);
    assert!(text(pending.backend().buffer()).contains("edit  saving"));

    fixture.app.acknowledge_persistence(sequence, false);
    let failed = draw(&mut fixture, 50, 6);
    assert!(text(failed.backend().buffer()).contains("edit  save failed"));
}

#[test]
fn mouse_can_create_focus_place_cursor_and_open_help() {
    let mut fixture = Fixture::new();
    let _empty = draw(&mut fixture, 40, 8);
    fixture.pointer(0, 0, PointerKind::Down(PointerButton::Left));
    assert!(matches!(
        fixture.app.state.mode,
        proqi::application::InteractionMode::Edit { .. }
    ));
    fixture.input(UiInput::Paste("A界B".to_owned()));
    fixture.input(UiInput::Key(UiKey::Escape));

    let _populated = draw(&mut fixture, 40, 8);
    fixture.pointer(3, 0, PointerKind::Down(PointerButton::Left));
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        proqi::domain::TextPosition::new(0, 1)
    );

    fixture.input(UiInput::Key(UiKey::Escape));
    let _board = draw(&mut fixture, 40, 8);
    fixture.pointer(35, 7, PointerKind::Down(PointerButton::Left));
    assert!(fixture.app.help);
}

#[test]
fn mouse_drag_reorders_thoughts_through_the_visible_gutter() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    let _board = draw(&mut fixture, 40, 10);
    fixture.pointer(0, 0, PointerKind::Down(PointerButton::Left));
    fixture.pointer(0, 2, PointerKind::Drag(PointerButton::Left));
    fixture.pointer(0, 2, PointerKind::Up(PointerButton::Left));

    let contents = fixture
        .app
        .state
        .board
        .live_thoughts()
        .iter()
        .map(|thought| thought.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, ["second", "third", "first"]);
}

#[test]
fn keyboard_selection_is_logical_and_visible() {
    let mut fixture = Fixture::new();
    fixture.paste("A界B");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: true,
    }));
    let snapshot = fixture.app.editor_snapshot().expect("editor");
    assert_eq!(
        snapshot.selection,
        Some(proqi::ports::editor::TextSelection {
            start: proqi::domain::TextPosition::new(0, 2),
            end: proqi::domain::TextPosition::new(0, 3),
        })
    );
    assert_eq!(
        snapshot.visual_lines[0]
            .selected_cells
            .expect("cells")
            .start,
        3
    );

    let terminal = draw(&mut fixture, 20, 5);
    let selected = terminal.backend().buffer()[(5, 0)].modifier;
    assert!(selected.contains(ratatui_core::style::Modifier::REVERSED));
}

#[test]
fn command_palette_is_searchable_and_mouse_operable() {
    let mut fixture = Fixture::new();
    fixture.input(UiInput::Key(UiKey::Character('/')));
    for character in "quit".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let terminal = draw(&mut fixture, 40, 12);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("/quit"));
    assert!(rendered.contains("Quit Proqi"));
    assert!(!rendered.contains("New thought"));

    fixture.pointer(2, 5, PointerKind::Down(PointerButton::Left));
    assert!(fixture.app.quit);
}

#[test]
fn remapped_board_binding_changes_behavior_and_visible_hint() {
    let mut settings = UiSettings::default();
    settings.keybindings.new = 't';
    let mut fixture = Fixture::with_settings(settings);
    fixture.input(UiInput::Key(UiKey::Character('n')));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Character('t')));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    fixture.input(UiInput::Key(UiKey::Escape));
    let terminal = draw(&mut fixture, 50, 6);
    assert!(text(terminal.backend().buffer()).contains("t new"));
}

#[test]
fn long_thought_cap_expands_without_changing_content() {
    let mut fixture = Fixture::new();
    let content = (0..20)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    fixture.paste(&content);
    fixture.input(UiInput::Key(UiKey::Escape));
    let initial = fixture.app.prepare_frame(Rect::new(0, 0, 40, 13));
    let thought = initial.thoughts.first().expect("thought");
    assert!(thought.hidden_rows > 0);
    let capped_height = thought.area.height;
    let overflow = thought.overflow.expect("overflow");

    fixture.pointer(
        overflow.x,
        overflow.y,
        PointerKind::Down(PointerButton::Left),
    );
    let expanded = fixture.app.prepare_frame(Rect::new(0, 0, 40, 13));
    assert!(expanded.thoughts[0].area.height > capped_height);
    assert_eq!(fixture.app.state.board.live_thoughts()[0].content, content);
}

#[test]
fn viewport_matrix_keeps_focus_visible_and_hit_geometry_current() {
    let mut fixture = Fixture::new();
    for index in 0..10 {
        fixture.paste(&format!("thought {index} 界"));
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    let focused = fixture.app.state.focused_thought.expect("focus");
    for (width, height) in [(6, 3), (120, 4), (18, 30), (9, 5), (80, 24)] {
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, width, height));
        let thought = layout.thought(focused).expect("focused thought visible");
        assert!(thought.area.right() <= width);
        assert!(thought.area.bottom() <= height.saturating_sub(1));
        assert_eq!(
            layout.hit_test(thought.gutter.x, thought.gutter.y),
            Some(proqi::ui::HitTarget::DragHandle(focused))
        );
        if thought.text_area.width > 0 {
            assert_eq!(
                layout.hit_test(thought.text_area.x, thought.text_area.y),
                Some(proqi::ui::HitTarget::Thought(focused))
            );
        }
    }
}

#[test]
fn narrow_empty_board_has_a_complete_explicit_buffer_snapshot() {
    let mut fixture = Fixture::new();
    let terminal = draw(&mut fixture, 12, 3);
    assert_eq!(
        text(terminal.backend().buffer()),
        "  +  create \n            \n[u][/][?][q]"
    );
}

#[test]
fn board_cut_waits_for_clipboard_success_and_copy_preserves_exact_content() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste(" exact\r\n界 ");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));

    let copy = fixture.effects(UiInput::Key(UiKey::Copy));
    let [
        Effect::WriteClipboard {
            request_id,
            intent: ClipboardIntent::Copy,
            content,
            ..
        },
    ] = copy.as_slice()
    else {
        panic!("expected copy effect");
    };
    assert_eq!(content, " exact\r\n界 ");
    assert!(
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock)
            .is_empty()
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    let failed_cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = failed_cut.as_slice() else {
        panic!("expected cut effect");
    };
    let failure = fixture.app.complete_clipboard_write(
        *request_id,
        Err(FailureCode::ClipboardFailed),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert!(matches!(failure.as_slice(), [Effect::Notify { .. }]));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [
        Effect::WriteClipboard {
            request_id,
            intent: ClipboardIntent::Cut,
            ..
        },
    ] = cut.as_slice()
    else {
        panic!("expected cut effect");
    };
    let deletion =
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock);
    assert!(matches!(
        deletion.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn editor_cut_is_non_destructive_on_failure_or_changed_selection() {
    let mut fixture = Fixture::new();
    fixture.paste("A界B");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: true,
    }));
    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [
        Effect::WriteClipboard {
            request_id,
            content,
            ..
        },
    ] = cut.as_slice()
    else {
        panic!("expected selection write");
    };
    assert_eq!(content, "B");
    fixture.app.complete_clipboard_write(
        *request_id,
        Err(FailureCode::ClipboardFailed),
        &mut fixture.ids,
        &fixture.clock,
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "A界B"
    );

    let cut = fixture.effects(UiInput::Key(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("expected selection write");
    };
    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    assert!(
        fixture
            .app
            .complete_clipboard_write(*request_id, Ok(()), &mut fixture.ids, &fixture.clock)
            .is_empty()
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").content,
        "A界B"
    );
}

#[test]
fn empty_or_failed_clipboard_read_never_creates_a_thought() {
    let mut fixture = Fixture::new();
    for result in [Ok(String::new()), Err(FailureCode::ClipboardFailed)] {
        let read = fixture.effects(UiInput::Key(UiKey::PasteClipboard));
        let [Effect::ReadClipboard { request_id }] = read.as_slice() else {
            panic!("expected clipboard read");
        };
        assert!(
            fixture
                .app
                .complete_clipboard_read(*request_id, result, &mut fixture.ids, &fixture.clock)
                .is_empty()
        );
        assert!(fixture.app.state.board.live_thoughts().is_empty());
    }
}

#[test]
fn storage_failure_blocks_new_edits_and_exposes_retry() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("durable candidate");
    fixture.app.acknowledge_persistence(sequence, false);
    let before = fixture.app.editor_snapshot().expect("editor");
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('x')))
            .is_empty()
    );
    assert_eq!(fixture.app.editor_snapshot().expect("editor"), before);
    assert_eq!(
        fixture.effects(UiInput::Key(UiKey::Character('r'))),
        vec![Effect::RetryPersistence { sequence }]
    );
}
