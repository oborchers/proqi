use crate::{
    adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
    application::AppState,
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{environment::IdGenerator as _, runtime::CaptureOwnerInfo},
    ui::{BoardApp, Theme, ThemePreference, render},
};
use ratatui_core::{backend::TestBackend, terminal::Terminal};

#[path = "tests/behavior.rs"]
mod behavior;

#[test]
fn takeover_overlay_has_a_complete_wide_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_takeover_wide", takeover_snapshot(82, 16));
    });
}

#[test]
fn takeover_overlay_has_a_complete_narrow_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_takeover_narrow", takeover_snapshot(38, 12));
    });
}

#[test]
fn takeover_overlay_has_a_complete_shallow_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_takeover_shallow", takeover_snapshot(62, 6));
    });
}

#[test]
fn listening_indicator_is_present_without_permanent_status_chrome() {
    let (mut app, _) = app_with_thought();
    app.screenshot_started();
    app.status = None;
    let snapshot = render_snapshot(&mut app, 72, 10);
    assert!(snapshot.contains("inbox listening"));
    assert!(!snapshot.contains("Screenshot Inbox is listening"));
}

fn takeover_snapshot(width: u16, height: u16) -> String {
    let (mut app, mut ids) = app_with_thought();
    let session_id = app.state.board.session.id;
    app.screenshot_conflict(CaptureOwnerInfo {
        instance_id: ids.instance_id(),
        session_id,
        pid: 42,
        version: "test".to_owned(),
        capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
        control_protocol: crate::ports::control::CONTROL_PROTOCOL_VERSION,
        control_endpoint: "private-control-endpoint".to_owned(),
        started_at: Timestamp::from_millis(1),
    });
    render_snapshot(&mut app, width, height)
}

fn app_with_thought() -> (BoardApp, FakeIdGenerator) {
    let mut ids = FakeIdGenerator::new(1_725_260_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("screenshot-ui"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought = Thought::new(
        ids.thought_id(),
        session.id,
        "active".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    (BoardApp::new(AppState::new(board), RopeEditorFactory), ids)
}

fn render_snapshot(app: &mut BoardApp, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let content = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{row:02}│{}│", content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
