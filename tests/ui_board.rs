//! Deterministic complete-board rendering and interaction contracts.

use proqi::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, ClipboardIntent, Effect, FailureCode},
    domain::{
        ContentAnnotation, ContentAnnotationKind, OperationSequence, Session, SessionBoard,
        Timestamp,
    },
    ports::{editor::CursorMovement, environment::IdGenerator},
    ui::{
        BoardApp, HitTarget, PastePayload, PointerButton, PointerInput, PointerKind, Theme,
        ThemePreference, UiInput, UiKey, UiSettings, render,
    },
};
use ratatui_core::{
    backend::{Backend, TestBackend},
    buffer::Buffer,
    layout::Rect,
    terminal::Terminal,
};

#[path = "support/snapshots.rs"]
mod snapshot_support;

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
            std::env::temp_dir().join("proqi-ui-contract"),
            Timestamp::from_millis(10),
        )
        .expect("session");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        Self {
            app: BoardApp::with_settings(
                AppState::new(board),
                settings,
                proqi::adapters::editor::RopeEditorFactory,
            ),
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
        self.input(UiInput::Pointer(PointerInput {
            column,
            row,
            kind,
            extend_selection: false,
        }));
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
    draw_theme(fixture, width, height, ThemePreference::Auto)
}

fn draw_theme(
    fixture: &mut Fixture,
    width: u16,
    height: u16,
    preference: ThemePreference,
) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = fixture.app.prepare_frame(frame.area());
            render(
                frame,
                &fixture.app,
                &layout,
                &Theme::resolve(preference, true),
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
    assert!(rendered.contains("+ Start typing"));
    assert!(rendered.contains("0 thoughts · saved"));
    assert!(!rendered.contains("compose"));
    assert!(rendered.contains("Esc Board"));

    fixture.input(UiInput::Key(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8));
    let help = layout
        .controls
        .iter()
        .find_map(|(target, area)| (*target == HitTarget::Help).then_some(*area))
        .expect("help control");
    fixture.pointer(help.x, help.y, PointerKind::Down(PointerButton::Left));
    let terminal = draw(&mut fixture, 40, 8);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("proqi shortcuts"));
    assert!(rendered.contains("Select all"));
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
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 10)).thoughts[0].text_area;
    assert_eq!(cursor.y, area.y + 2);
    assert_eq!(cursor.x, 8);

    fixture.app.acknowledge_persistence(sequence, true);
    let terminal = draw(&mut fixture, 40, 10);
    assert!(text(terminal.backend().buffer()).contains("edit · saved"));
}

#[test]
fn cursor_uses_expanded_tab_cells() {
    let mut fixture = Fixture::new();
    fixture.paste("a\tb");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeBack,
        extend_selection: false,
    }));
    let mut terminal = draw(&mut fixture, 20, 5);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor position");
    assert_eq!(cursor.x, 6);
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 20, 5)).thoughts[0].text_area;
    assert_eq!(cursor.y, area.y);
}

#[test]
fn exact_wrap_boundary_keeps_the_terminal_cursor_visible() {
    let mut fixture = Fixture::new();
    fixture.paste("123456");
    let mut terminal = draw(&mut fixture, 8, 5);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor at wrapped document end");
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 8, 5)).thoughts[0].text_area;
    assert_eq!((cursor.x, cursor.y), (area.x, area.y + 1));
}

#[test]
fn trailing_newlines_each_receive_their_own_terminal_cursor_row() {
    let mut fixture = Fixture::new();
    fixture.paste("line");
    fixture.input(UiInput::Key(UiKey::Enter));

    let area = fixture.app.prepare_frame(Rect::new(0, 0, 20, 8)).thoughts[0].text_area;
    let mut terminal = draw(&mut fixture, 20, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor after first newline");
    assert_eq!((cursor.x, cursor.y), (area.x, area.y + 1));

    fixture.input(UiInput::Key(UiKey::Enter));
    let mut terminal = draw(&mut fixture, 20, 8);
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .expect("cursor after second newline");
    assert_eq!((cursor.x, cursor.y), (area.x, area.y + 2));
}

#[test]
fn board_rendering_uses_the_editor_wrap_model_without_clipping_words() {
    let mut fixture = Fixture::new();
    fixture.paste("aaaaaa bbbbbb cccccc dddddd");
    fixture.input(UiInput::Key(UiKey::Escape));
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 12, 10));
    assert!(layout.thoughts[0].area.height >= 3);
    let terminal = draw(&mut fixture, 12, 10);
    let rendered = text(terminal.backend().buffer());
    for word in ["aaaaaa", "bbbbbb", "cccccc", "dddddd"] {
        assert!(rendered.contains(word), "missing {word} in:\n{rendered}");
    }
    assert!(!rendered.contains("aaaaaa bbb"));
}

#[test]
fn repeated_resize_preserves_content_and_logical_cursor() {
    let mut fixture = Fixture::new();
    fixture.paste("one 👩‍💻 two combining e\u{301} three 第二行 four five six");
    fixture.input(UiInput::Key(UiKey::Move {
        movement: CursorMovement::DocumentStart,
        extend_selection: false,
    }));
    for _ in 0..12 {
        fixture.input(UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeForward,
            extend_selection: true,
        }));
    }
    let original = fixture.app.editor_snapshot().expect("editor snapshot");

    for (width, height) in [(12, 4), (80, 5), (20, 12), (8, 3), (40, 8)] {
        let terminal = draw(&mut fixture, width, height);
        assert_eq!(terminal.backend().buffer().area.width, width);
        assert_eq!(terminal.backend().buffer().area.height, height);
    }

    let resized = fixture.app.editor_snapshot().expect("editor snapshot");
    assert_eq!(resized.content, original.content);
    assert_eq!(resized.cursor, original.cursor);
    assert_eq!(resized.selection, original.selection);
    assert!(resized.scroll_row < resized.visual_lines.len());
}

#[test]
fn pending_and_failed_durability_are_visibly_distinct() {
    let mut fixture = Fixture::new();
    let sequence = fixture.paste("important unsaved text");
    let pending = draw(&mut fixture, 50, 6);
    assert!(text(pending.backend().buffer()).contains("edit · saving"));

    fixture.app.acknowledge_persistence(sequence, false);
    let failed = draw(&mut fixture, 50, 6);
    let failed_text = text(failed.backend().buffer());
    assert!(failed_text.contains("save failed · r Retry · w Export recovery"));
    assert!(failed_text.contains("edit · unsaved"));
}

#[test]
fn mouse_can_create_focus_place_cursor_and_open_help() {
    let mut fixture = Fixture::new();
    let _empty = draw(&mut fixture, 40, 8);
    let prompt = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 40, 8))
        .insert
        .expect("compose prompt");
    fixture.pointer(
        prompt.x.saturating_add(prompt.width / 2),
        prompt.y,
        PointerKind::Down(PointerButton::Left),
    );
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Compose
    );
    assert!(fixture.app.compose_editor_visible());
    let mut engaged = draw(&mut fixture, 40, 8);
    assert!(engaged.backend_mut().get_cursor_position().is_ok());
    fixture.input(UiInput::Paste("A界B".to_owned()));
    fixture.input(UiInput::Key(UiKey::Escape));

    let _populated = draw(&mut fixture, 40, 8);
    let text_area = fixture.app.prepare_frame(Rect::new(0, 0, 40, 8)).thoughts[0].text_area;
    fixture.pointer(
        text_area.x.saturating_add(1),
        text_area.y,
        PointerKind::Down(PointerButton::Left),
    );
    assert_eq!(
        fixture.app.editor_snapshot().expect("editor").cursor,
        proqi::domain::TextPosition::new(0, 1)
    );

    fixture.input(UiInput::Key(UiKey::Escape));
    let _board = draw(&mut fixture, 40, 8);
    let help = fixture
        .app
        .prepare_frame(Rect::new(0, 0, 40, 8))
        .controls
        .into_iter()
        .find_map(|(target, area)| (target == proqi::ui::HitTarget::Help).then_some(area))
        .expect("help control");
    fixture.pointer(help.x, help.y, PointerKind::Down(PointerButton::Left));
    assert!(fixture.app.help);
}

#[test]
fn mouse_drag_reorders_thoughts_through_the_visible_gutter() {
    let mut fixture = Fixture::new();
    for content in ["first", "second", "third"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    let board = draw(&mut fixture, 40, 10);
    let rendered = text(board.backend().buffer());
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 40, 10));
    let separator = layout.thoughts[1]
        .separator_before
        .expect("separator geometry");
    assert!(
        rendered
            .lines()
            .nth(usize::from(separator.y))
            .expect("separator row")
            .starts_with("  ─")
    );
    assert_eq!(layout.hit_test(separator.x, separator.y), None);
    let target_row = layout.thoughts[2].gutter.y;
    let source_row = layout.thoughts[0].gutter.y;
    fixture.pointer(0, source_row, PointerKind::Down(PointerButton::Left));
    fixture.pointer(0, target_row, PointerKind::Drag(PointerButton::Left));
    fixture.pointer(0, target_row, PointerKind::Up(PointerButton::Left));

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
    let area = fixture.app.prepare_frame(Rect::new(0, 0, 20, 5)).thoughts[0].text_area;
    let selected = terminal.backend().buffer()[(area.x + 3, area.y)].modifier;
    assert!(selected.contains(ratatui_core::style::Modifier::REVERSED));
}

#[test]
fn thought_search_filters_content_and_focuses_the_selected_match() {
    let mut fixture = Fixture::new();
    fixture.paste("first searchable prompt");
    let first = fixture.app.state.focused_thought.expect("first thought");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.paste("unrelated second prompt");
    fixture.input(UiInput::Key(UiKey::Escape));

    fixture.input(UiInput::Key(UiKey::Character('/')));
    for character in "searchable".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let terminal = draw(&mut fixture, 40, 10);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("/searchable"));
    assert!(rendered.contains("first searchable prompt"));
    let (_, results, _) = fixture.app.search_view().expect("search view");
    assert_eq!(results, ["first searchable prompt"]);

    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(fixture.app.state.focused_thought, Some(first));
    assert!(fixture.app.search_view().is_none());
}

#[path = "ui_board/agent.rs"]
mod agent;
#[path = "ui_board/agent_direct.rs"]
mod agent_direct;
#[path = "ui_board/agent_discovery.rs"]
mod agent_discovery;
#[path = "ui_board/agent_hermes.rs"]
mod agent_hermes;
#[path = "ui_board/agent_pi.rs"]
mod agent_pi;
#[path = "ui_board/agent_selection.rs"]
mod agent_selection;
#[path = "ui_board/agent_session.rs"]
mod agent_session;
#[path = "ui_board/annotations.rs"]
mod annotations;
#[path = "ui_board/attachment_accessibility.rs"]
mod attachment_accessibility;
#[path = "ui_board/blank.rs"]
mod blank;
#[path = "ui_board/clipboard.rs"]
mod clipboard;
#[path = "ui_board/compose.rs"]
mod compose;
#[path = "ui_board/composition.rs"]
mod composition;
#[path = "ui_board/delete_alias.rs"]
mod delete_alias;
#[path = "ui_board/direction_modifier_parity.rs"]
mod direction_modifier_parity;
#[path = "ui_board/durability.rs"]
mod durability;
#[path = "ui_board/fast_navigation.rs"]
mod fast_navigation;
#[path = "ui_board/insertion_navigation.rs"]
mod insertion_navigation;
#[path = "ui_board/kilo.rs"]
mod kilo;
#[path = "ui_board/modifier_parity.rs"]
mod modifier_parity;
#[path = "ui_board/movement_symmetry.rs"]
mod movement_symmetry;
#[path = "ui_board/navigation.rs"]
mod navigation;
#[path = "ui_board/palette.rs"]
mod palette;
#[path = "ui_board/placeholder_entry.rs"]
mod placeholder_entry;
#[path = "ui_board/pointer_selection.rs"]
mod pointer_selection;
#[path = "ui_board/scroll_regressions.rs"]
mod scroll_regressions;
#[path = "ui_board/select_all.rs"]
mod select_all;
#[path = "ui_board/selection.rs"]
mod selection;
#[path = "ui_board/session_navigation.rs"]
mod session_navigation;
#[path = "ui_board/smart_lists.rs"]
mod smart_lists;
#[path = "ui_board/snapshots.rs"]
mod snapshots;
#[path = "ui_board/submission_locks.rs"]
mod submission_locks;
#[path = "ui_board/submit_all.rs"]
mod submit_all;
#[path = "ui_board/top_boundary_snapshots.rs"]
mod top_boundary_snapshots;
