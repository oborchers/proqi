use crate::{
    adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
    application::AppState,
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{
        environment::IdGenerator as _, runtime::CaptureOwnerInfo,
        screenshot::ScreenshotActivityPolicy,
    },
    ui::{BoardApp, Theme, ThemePreference, render},
};
use ratatui_core::{backend::TestBackend, terminal::Terminal};
use std::time::Duration;

#[path = "tests/activity.rs"]
mod activity;
#[path = "tests/admission.rs"]
mod admission;
#[path = "tests/barrier.rs"]
mod barrier;
#[path = "tests/behavior.rs"]
mod behavior;
#[path = "tests/focus.rs"]
mod focus;
#[path = "tests/lifecycle.rs"]
mod lifecycle;
#[path = "tests/paging.rs"]
mod paging;

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
fn takeover_list_uses_identical_arrow_and_jk_navigation() {
    for (arrow, vim) in [
        (
            crate::ui::UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                extend_selection: true,
            },
            crate::ui::UiKey::PrimaryCharacter('J'),
        ),
        (
            crate::ui::UiKey::PrimaryShiftMove {
                movement: crate::ports::editor::CursorMovement::DocumentStart,
            },
            crate::ui::UiKey::Character('k'),
        ),
    ] {
        let (mut arrow_app, mut arrow_ids) = app_with_thought();
        let (mut vim_app, mut vim_ids) = app_with_thought();
        let session_id = arrow_app.state.board.session.id;
        let owner = CaptureOwnerInfo {
            instance_id: arrow_ids.instance_id(),
            session_id,
            pid: 42,
            version: "test".to_owned(),
            capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
            control_protocol: crate::ports::control::CONTROL_PROTOCOL_VERSION,
            control_endpoint: "private-control-endpoint".to_owned(),
            started_at: Timestamp::from_millis(1),
        };
        arrow_app.screenshot_conflict(owner.clone());
        vim_app.screenshot_conflict(owner);
        let clock = crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2));
        arrow_app.handle(crate::ui::UiInput::Key(arrow), &mut arrow_ids, &clock);
        vim_app.handle(crate::ui::UiInput::Key(vim), &mut vim_ids, &clock);
        assert_eq!(
            arrow_app.screenshot_takeover_view(),
            vim_app.screenshot_takeover_view()
        );
    }
}

#[test]
fn global_quit_precedes_screenshot_takeover_navigation() {
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
    let clock = crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2));
    assert!(
        app.handle(
            crate::ui::UiInput::Key(crate::ui::UiKey::Quit),
            &mut ids,
            &clock
        )
        .is_empty()
    );
    assert!(app.quit);
}

#[test]
fn listening_indicator_is_present_without_permanent_status_chrome() {
    let (mut app, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.status = None;
    let snapshot = render_snapshot(&mut app, 72, 10);
    assert!(snapshot.contains("inbox listening"));
    assert!(!snapshot.contains("Screenshot Inbox is listening"));
}

#[test]
fn releasing_inbox_has_a_complete_wide_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_releasing_wide", releasing_snapshot(82, 12));
    });
}

#[test]
fn releasing_inbox_has_a_complete_narrow_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_releasing_narrow", releasing_snapshot(38, 10));
    });
}

#[test]
fn releasing_inbox_has_a_complete_shallow_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_releasing_shallow", releasing_snapshot(62, 6));
    });
}

#[test]
fn paused_inbox_has_a_persistent_wide_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_paused_wide", paused_snapshot(82, 12));
    });
}

#[test]
fn paused_inbox_has_a_persistent_narrow_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_paused_narrow", paused_snapshot(38, 10));
    });
}

#[test]
fn paused_inbox_has_a_persistent_shallow_snapshot() {
    insta::with_settings!({snapshot_path => "../../snapshots"}, {
        insta::assert_snapshot!("screenshot_paused_shallow", paused_snapshot(62, 6));
    });
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

fn paused_snapshot(width: u16, height: u16) -> String {
    let (mut app, _) = app_with_thought();
    app.configure_screenshot_activity(ScreenshotActivityPolicy::new(20, 10).expect("pause policy"));
    app.screenshot_started(Duration::ZERO);
    app.advance_screenshot_activity(Duration::from_secs(20 * 60));
    app.screenshot_stopped();
    render_snapshot(&mut app, width, height)
}

fn releasing_snapshot(width: u16, height: u16) -> String {
    let (mut app, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.screenshot_stopping_completed();
    app.status = None;
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
