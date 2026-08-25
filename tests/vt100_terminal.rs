//! Escape-stream coverage between deterministic buffers and real PTYs.

use proqi::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::AppState,
    domain::{Session, SessionBoard, Timestamp},
    ports::environment::IdGenerator as _,
    ui::{BoardApp, Theme, ThemePreference, UiInput, UiKey, UiSettings, render},
};
use ratatui_core::{
    layout::Rect,
    terminal::{Terminal, TerminalOptions, Viewport},
};
use ratatui_crossterm::CrosstermBackend;

#[test]
fn emitted_escape_stream_reflows_without_stale_cells() {
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-vt100-contract"),
        Timestamp::from_millis(10),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    let mut app = BoardApp::with_settings(
        AppState::new(board),
        UiSettings::default(),
        RopeEditorFactory,
    );
    let clock = FakeClock::new(Timestamp::from_millis(20));
    let _effects = app.handle(
        UiInput::Paste("Review https://github.com/oborchers/proqi".to_owned()),
        &mut ids,
        &clock,
    );
    let _effects = app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);

    let mut bytes = Vec::new();
    {
        let backend = CrosstermBackend::new(&mut bytes);
        let options = TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 48, 9)),
        };
        let mut terminal = Terminal::with_options(backend, options).expect("terminal");
        terminal
            .draw(|frame| {
                let layout = app.prepare_frame(frame.area());
                render(
                    frame,
                    &app,
                    &layout,
                    &Theme::resolve(ThemePreference::Dark, true),
                );
            })
            .expect("draw board");
    }

    let mut parser = vt100::Parser::new(9, 48, 0);
    parser.process(&bytes);
    let screen = parser.screen();
    let contents = screen.contents();
    assert!(
        contents.contains("Review https://github.com"),
        "{contents:?}"
    );
    assert!(contents.contains("+ New thought"), "{contents:?}");
    assert!(!contents.contains('\u{1b}'));
}
