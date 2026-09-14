//! Automatic-pause explanation acknowledgement and status precedence.

use std::time::Duration;

use ratatui_core::layout::Rect;

use super::behavior::{app_with_thought, candidate, next_commit};
use crate::{
    application::{DurabilityState, Effect, FailureCode, InteractionMode, ScreenshotIntent},
    domain::{OperationSequence, Timestamp},
    ports::{
        agent::AgentError,
        editor::CursorMovement,
        screenshot::{ScreenshotActivityPolicy, ScreenshotError},
        store::StoreError,
    },
    ui::{BoardApp, PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};

const INACTIVITY_NOTICE: &str = "Screenshot Inbox paused after 1 minute without activity";
const CAPTURE_NOTICE: &str = "Screenshot Inbox paused after 2 unattended captures";

#[test]
fn inactivity_boundary_and_boundary_plus_one_pause_exactly_once() {
    for elapsed in [60, 61] {
        let (mut app, _, _, _) = app_with_thought();
        configure(&mut app);
        app.screenshot_started(Duration::ZERO);
        assert!(
            app.advance_screenshot_activity(Duration::from_secs(59))
                .is_empty()
        );
        assert_eq!(
            app.advance_screenshot_activity(Duration::from_secs(elapsed)),
            vec![Effect::Screenshot(ScreenshotIntent::Disable)]
        );
        assert!(
            app.advance_screenshot_activity(Duration::from_secs(elapsed + 1))
                .is_empty()
        );
        app.screenshot_stopped();
        assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
    }
}

#[test]
fn capture_boundary_and_boundary_plus_one_keep_only_the_bounded_prefix() {
    for offered in [2, 3] {
        let (mut app, _, _, _) = app_with_thought();
        configure(&mut app);
        app.screenshot_started(Duration::ZERO);
        let effects = app.queue_screenshot_candidates(
            (0..offered).map(|index| candidate(80 + u8::try_from(index).expect("small index"))),
        );
        assert_eq!(effects, vec![Effect::Screenshot(ScreenshotIntent::Disable)]);
        assert_eq!(app.screenshot.candidates.len(), 2);
        app.screenshot_stopped();
        assert_eq!(app.status_text(), Some(CAPTURE_NOTICE));
    }
}

#[test]
fn keyboard_navigation_selection_edit_typing_and_paste_acknowledge_without_resuming() {
    let cases = [
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        UiInput::Key(UiKey::UnmodifiedSpace),
        UiInput::Key(UiKey::Enter),
        UiInput::Paste("new thought".to_owned()),
    ];
    for input in cases {
        let (mut app, mut ids, clock, _) = paused_inactivity();
        app.handle(input, &mut ids, &clock);
        assert_acknowledged_and_paused(&app, "keyboard or paste");
    }

    let (mut app, mut ids, clock, thought_id) = paused_inactivity();
    app.state.mode = InteractionMode::Edit { thought_id };
    app.sync_editor_from_state();
    app.handle(UiInput::Key(UiKey::Character('x')), &mut ids, &clock);
    assert_eq!(app.editor_snapshot().expect("editor").content, "activex");
    assert_acknowledged_and_paused(&app, "typing");
}

#[test]
fn compose_typing_and_paste_acknowledge_without_losing_the_admitted_input() {
    for input in [
        UiInput::Key(UiKey::Character('x')),
        UiInput::Paste("pasted".to_owned()),
    ] {
        let (mut app, mut ids, clock) = empty_paused_app();
        app.handle(input, &mut ids, &clock);
        let content = app.editor_snapshot().expect("materialized editor").content;
        assert!(content == "x" || content == "pasted");
        assert_acknowledged_and_paused(&app, "compose input");
    }
}

#[test]
fn board_click_drag_and_scroll_acknowledge_even_when_the_board_does_not_mutate() {
    for kind in [
        PointerKind::Down(PointerButton::Left),
        PointerKind::Drag(PointerButton::Left),
        PointerKind::ScrollDown,
    ] {
        let (mut app, mut ids, clock, _) = paused_inactivity();
        let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
        let focused = layout.thoughts[0].text_area;
        let before = app.state.board.live_thoughts().len();
        app.handle(
            UiInput::Pointer(PointerInput {
                column: focused.x,
                row: focused.y,
                kind,
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        );
        assert_eq!(app.state.board.live_thoughts().len(), before);
        assert_acknowledged_and_paused(&app, "pointer input");
    }
}

#[test]
fn both_pause_causes_share_the_same_next_interaction_lifecycle() {
    let (mut inactivity, mut inactivity_ids, inactivity_clock, _) = paused_inactivity();
    inactivity.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut inactivity_ids,
        &inactivity_clock,
    );
    assert_acknowledged_and_paused(&inactivity, "inactivity");

    let (mut captures, mut capture_ids, capture_clock, _) = paused_for_capture_limit();
    captures.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut capture_ids,
        &capture_clock,
    );
    assert_acknowledged_and_paused(&captures, "capture limit");
    assert_eq!(
        captures.screenshot_footer_state(false).as_deref(),
        Some("inbox paused · 2 captures")
    );
}

#[test]
fn interaction_during_inactivity_stop_acknowledges_before_warning_presentation() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    configure(&mut app);
    app.screenshot_started(Duration::ZERO);
    assert_eq!(
        app.advance_screenshot_activity(Duration::from_secs(60)),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    app.screenshot_stopped();
    assert_acknowledged_and_paused(&app, "inactivity stop interaction");
}

#[test]
fn modal_input_waits_for_the_next_board_interaction_to_acknowledge() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.open_palette();
    pause_for_inactivity(&mut app);
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
    app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);
    assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_acknowledged_and_paused(&app, "post-modal board input");
}

#[test]
fn commands_board_execution_acknowledges_but_modal_navigation_does_not() {
    let (mut app, mut ids, clock, thought_id) = app_with_thought();
    app.open_palette();
    pause_for_inactivity(&mut app);
    app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
    app.handle(
        UiInput::Key(UiKey::Shortcut(crate::ui::ShortcutActionId::Edit)),
        &mut ids,
        &clock,
    );
    assert_eq!(app.state.mode, InteractionMode::Edit { thought_id });
    assert_acknowledged_and_paused(&app, "Commands Edit execution");
}

#[test]
fn warning_created_by_the_acknowledging_action_wins() {
    let (mut app, mut ids, clock, _) = paused_inactivity();
    app.handle(
        UiInput::Key(UiKey::Shortcut(
            crate::ui::ShortcutActionId::RetryScreenshotCapture,
        )),
        &mut ids,
        &clock,
    );
    assert_eq!(app.status_text(), Some("No failed capture to retry"));
    assert!(!app.screenshot_listening());
}

#[test]
fn queued_input_at_capture_limit_replays_once_and_acknowledges_before_release() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.configure_screenshot_activity(
        ScreenshotActivityPolicy::new(1, 1).expect("valid activity policy"),
    );
    app.screenshot_started(Duration::ZERO);
    assert_eq!(
        app.queue_screenshot_candidates([candidate(95)]),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    let capture = next_commit(&mut app, &mut ids, &clock);
    assert!(
        app.handle(UiInput::Paste("queued once".to_owned()), &mut ids, &clock)
            .is_empty()
    );
    assert_eq!(app.state.board.live_thoughts().len(), 1);
    app.complete_screenshot_capture(Ok(super::behavior::created(&capture)), &mut ids, &clock);
    assert_eq!(
        app.state
            .board
            .live_thoughts()
            .iter()
            .filter(|thought| thought.content == "queued once")
            .count(),
        1
    );
    app.screenshot_stopped();
    assert_acknowledged_and_paused(&app, "capture barrier replay");
}

#[test]
fn passive_transport_render_timer_watcher_and_unrelated_completion_do_not_acknowledge() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.refresh_agents();
    pause_for_inactivity(&mut app);
    for input in [
        UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Move,
            extend_selection: false,
        }),
        UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Up(PointerButton::Left),
            extend_selection: false,
        }),
        UiInput::Resize {
            width: 40,
            height: 8,
        },
        UiInput::HostFocusGained,
        UiInput::HostFocusLost,
    ] {
        app.handle(input, &mut ids, &clock);
        assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
    }
    let _layout = app.prepare_frame(Rect::new(0, 0, 38, 8));
    assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
    assert!(
        app.advance_screenshot_activity(Duration::from_secs(120))
            .is_empty()
    );
    assert!(app.queue_screenshot_candidates([candidate(90)]).is_empty());
    app.set_success("unrelated success");
    app.set_warning("unrelated warning");
    app.set_attachment_warning("unrelated attachment warning");
    app.complete_agent_discovery(Err(AgentError::Unavailable(
        "unrelated discovery failure".to_owned(),
    )));
    assert_eq!(app.status_text(), Some(INACTIVITY_NOTICE));
}

#[test]
fn retained_capture_error_outlives_replayed_copy_completion() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.configure_screenshot_activity(
        ScreenshotActivityPolicy::new(1, 1).expect("valid activity policy"),
    );
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(96)]);
    let _capture = next_commit(&mut app, &mut ids, &clock);
    assert!(
        app.handle(UiInput::Key(UiKey::Copy), &mut ids, &clock)
            .is_empty()
    );
    let replay = app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    let request_id = replay
        .iter()
        .find_map(|effect| match effect {
            Effect::WriteClipboard { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .expect("replayed copy");
    let retained = app
        .status_text()
        .expect("retained capture error")
        .to_owned();
    app.complete_clipboard_write(request_id, Ok(()), &mut ids, &clock);
    assert_eq!(app.status_text(), Some(retained.as_str()));
    assert!(app.screenshot_retry_ready());
}

#[test]
fn persistence_recovery_capture_retry_and_quit_errors_outlive_acknowledging_input() {
    let (mut recovery, mut recovery_ids, recovery_clock, _) = app_with_thought();
    recovery.state.durability = DurabilityState::Failed {
        durable: OperationSequence::default(),
        failed: OperationSequence::new(1),
        code: FailureCode::StorageFailed,
    };
    recovery.set_storage_failure("critical storage failure");
    recovery.enter_screenshot_paused(crate::application::ScreenshotPauseReason::Inactivity {
        minutes: 1,
    });
    recovery.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut recovery_ids,
        &recovery_clock,
    );
    assert_eq!(recovery.status_text(), Some("critical storage failure"));

    let (mut capture, mut capture_ids, capture_clock, _) = app_with_thought();
    configure(&mut capture);
    capture.screenshot_started(Duration::ZERO);
    capture.queue_screenshot_candidates([candidate(91), candidate(92)]);
    let commit = next_commit(&mut capture, &mut capture_ids, &capture_clock);
    capture.complete_screenshot_capture(Err(StoreError::Busy), &mut capture_ids, &capture_clock);
    capture.screenshot_stopped();
    let retained = capture
        .status_text()
        .expect("retained capture error")
        .to_owned();
    assert!(retained.contains("Retry Screenshot Capture"));
    capture.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut capture_ids,
        &capture_clock,
    );
    assert_eq!(capture.status_text(), Some(retained.as_str()));
    assert!(capture.screenshot_retry_ready());
    assert_eq!(commit.source, candidate(91).fingerprint);

    capture.handle(UiInput::Key(UiKey::Quit), &mut capture_ids, &capture_clock);
    let quit = capture.status_text().expect("quit warning").to_owned();
    assert!(quit.contains("quit again to abandon"));
    capture.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::VisualDown,
            extend_selection: false,
        }),
        &mut capture_ids,
        &capture_clock,
    );
    assert_eq!(capture.status_text(), Some(quit.as_str()));
    assert!(
        capture
            .screenshot_failed(&ScreenshotError::Reconciliation)
            .is_empty()
    );
}

fn configure(app: &mut BoardApp) {
    app.configure_screenshot_activity(
        ScreenshotActivityPolicy::new(1, 2).expect("valid activity policy"),
    );
}

fn pause_for_inactivity(app: &mut BoardApp) {
    configure(app);
    app.screenshot_started(Duration::ZERO);
    assert_eq!(
        app.advance_screenshot_activity(Duration::from_secs(60)),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    assert!(matches!(
        app.screenshot_stopped().as_slice(),
        [Effect::NotifyScreenshotPause(_)]
    ));
}

fn paused_inactivity() -> (
    BoardApp,
    crate::adapters::memory::FakeIdGenerator,
    crate::adapters::memory::FakeClock,
    crate::domain::ThoughtId,
) {
    let (mut app, ids, clock, thought_id) = app_with_thought();
    pause_for_inactivity(&mut app);
    (app, ids, clock, thought_id)
}

fn paused_for_capture_limit() -> (
    BoardApp,
    crate::adapters::memory::FakeIdGenerator,
    crate::adapters::memory::FakeClock,
    crate::domain::ThoughtId,
) {
    let (mut app, ids, clock, thought_id) = app_with_thought();
    configure(&mut app);
    app.screenshot_started(Duration::ZERO);
    assert_eq!(
        app.queue_screenshot_candidates([candidate(93), candidate(94)]),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    app.screenshot_stopped();
    (app, ids, clock, thought_id)
}

fn empty_paused_app() -> (
    BoardApp,
    crate::adapters::memory::FakeIdGenerator,
    crate::adapters::memory::FakeClock,
) {
    use crate::{
        adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
        application::AppState,
        domain::{Session, SessionBoard},
        ports::environment::IdGenerator as _,
    };
    let mut ids = FakeIdGenerator::new(1_725_281_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("screenshot-warning-compose"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        RopeEditorFactory,
    );
    pause_for_inactivity(&mut app);
    (
        app,
        ids,
        crate::adapters::memory::FakeClock::new(Timestamp::from_millis(2)),
    )
}

fn assert_acknowledged_and_paused(app: &BoardApp, route: &str) {
    assert_eq!(app.status_text(), None, "{route}");
    assert!(!app.screenshot_listening(), "{route}");
    assert!(matches!(
        app.screenshot_palette_action(),
        super::super::ScreenshotPaletteAction::Resume
    ));
}
