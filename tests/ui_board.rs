//! Deterministic complete-board rendering and interaction contracts.

use std::path::PathBuf;

use proqi::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::AppState,
    domain::{OperationSequence, Session, SessionBoard, Timestamp},
    ports::{editor::TextViewport, environment::IdGenerator},
    ui::{BoardApp, UiInput, UiKey, render},
};
use ratatui_core::{
    backend::{Backend, TestBackend},
    buffer::Buffer,
    terminal::Terminal,
};

struct Fixture {
    app: BoardApp,
    ids: FakeIdGenerator,
    clock: FakeClock,
}

impl Fixture {
    fn new() -> Self {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            PathBuf::from("/tmp/proqi-ui-contract"),
            Timestamp::from_millis(10),
        )
        .expect("session");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        Self {
            app: BoardApp::new(AppState::new(board)),
            ids,
            clock: FakeClock::new(Timestamp::from_millis(20)),
        }
    }

    fn input(&mut self, input: UiInput) {
        let _effects = self.app.handle(input, &mut self.ids, &self.clock);
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
    fixture.app.prepare_layout(TextViewport::new(
        width.saturating_sub(4).max(1),
        height.saturating_sub(1).max(1),
    ));
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| render(frame, &fixture.app))
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
