use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, InteractionMode, ScreenshotIntent},
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{
        editor::CursorMovement,
        environment::IdGenerator as _,
        runtime::CaptureOwnerInfo,
        screenshot::{ScreenshotCandidate, ScreenshotFingerprint, ScreenshotImageType},
        store::{
            CaptureCommit, CaptureCommitOutcome, CaptureReceipt, CommitReceipt, DurableIdentity,
            StoreError,
        },
    },
    ui::{BoardApp, PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};
use ratatui_core::layout::Rect;
use std::time::Duration;

#[test]
fn durable_capture_preserves_an_active_editor_and_exact_path_annotation() {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.state.mode = InteractionMode::Edit {
        thought_id: original_id,
    };
    app.sync_editor_from_state();
    let editor_before = app.editor_snapshot().expect("editor");
    app.queue_screenshot_candidates([candidate(3)]);
    let commit = next_commit(&mut app, &mut ids, &clock);
    let effects = app.handle(UiInput::Key(UiKey::Character('!')), &mut ids, &clock);
    assert!(effects.is_empty());
    let editor_during = app.editor_snapshot().expect("live editor during commit");
    assert_eq!(editor_during, editor_before);
    app.complete_screenshot_capture(Ok(created(&commit)), &mut ids, &clock);

    assert_eq!(app.state.focused_thought, Some(original_id));
    assert_eq!(
        app.editor_snapshot().expect("replayed editor").content,
        "active!"
    );
    let captured = app.state.board.live_thoughts()[1];
    assert_eq!(captured.content, candidate(3).path.to_string_lossy());
    assert_eq!(captured.annotations[0].start, 0);
    assert_eq!(captured.annotations[0].end, captured.content.len());
    assert_eq!(app.status_text(), None);
}

#[test]
fn newest_capture_in_one_detection_burst_is_left_ready_for_annotation() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(4), candidate(5)]);

    let first = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Ok(created(&first)), &mut ids, &clock);
    let second = next_commit(&mut app, &mut ids, &clock);
    let newest_id = capture_thought_id(&second);
    app.complete_screenshot_capture(Ok(created(&second)), &mut ids, &clock);

    assert_eq!(app.state.focused_thought, Some(newest_id));
    assert_eq!(
        app.state.mode,
        InteractionMode::Edit {
            thought_id: newest_id
        }
    );
    assert_eq!(app.status_text(), Some("2 new captures"));
}

#[test]
fn separated_capture_feedback_restarts_at_one() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(6)]);
    let first = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Ok(created(&first)), &mut ids, &clock);
    app.state.mode = InteractionMode::Board;

    app.queue_screenshot_candidates([candidate(7)]);
    let second = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Ok(created(&second)), &mut ids, &clock);

    assert_eq!(app.status_text(), Some("1 new capture"));
}

#[test]
fn failed_capture_has_no_partial_thought_and_is_explicitly_retryable() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(8)]);
    let commit = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    assert_eq!(app.state.board.live_thoughts().len(), 1);
    let retry_effects = app.retry_screenshot_capture(&mut ids, &clock);
    let [Effect::CommitCapture(retry)] = retry_effects.as_slice() else {
        panic!("retry capture");
    };
    assert_eq!(retry.source, commit.source);
    assert_ne!(retry.operation.id, commit.operation.id);
}

#[test]
fn retry_rebuilds_after_an_intervening_durable_editor_change() {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(18)]);
    let failed = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);

    app.state.mode = InteractionMode::Edit {
        thought_id: original_id,
    };
    app.sync_editor_from_state();
    app.handle(UiInput::Key(UiKey::Character('x')), &mut ids, &clock);
    let revision_effects = app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);
    let [Effect::CommitRevision(revision)] = revision_effects.as_slice() else {
        panic!("editor revision");
    };
    app.acknowledge_persistence_result(revision.sequence, Ok(()));

    let retry_effects = app.retry_screenshot_capture(&mut ids, &clock);
    let [Effect::CommitCapture(retry)] = retry_effects.as_slice() else {
        panic!("fresh retry capture");
    };
    assert_eq!(retry.source, failed.source);
    assert!(retry.operation.sequence > failed.operation.sequence);
    assert_ne!(retry.operation.id, failed.operation.id);
    app.complete_screenshot_capture(Ok(created(retry)), &mut ids, &clock);
    assert_eq!(app.state.board.live_thoughts().len(), 2);
}

#[test]
fn failed_first_capture_in_a_burst_retries_then_drains_in_order() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(13), candidate(14)]);
    let first = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);

    let retry_effects = app.retry_screenshot_capture(&mut ids, &clock);
    let [Effect::CommitCapture(retry)] = retry_effects.as_slice() else {
        panic!("retry first capture");
    };
    assert_eq!(retry.source, first.source);
    app.complete_screenshot_capture(Ok(created(retry)), &mut ids, &clock);
    let second = next_commit(&mut app, &mut ids, &clock);
    assert_eq!(first.source, candidate(13).fingerprint);
    assert_eq!(second.source, candidate(14).fingerprint);
    app.complete_screenshot_capture(Ok(created(&second)), &mut ids, &clock);
    assert_eq!(app.state.board.live_thoughts().len(), 3);
}

#[test]
fn quit_during_capture_is_deferred_and_flushes_the_live_editor() {
    let (mut app, mut ids, clock, original_id) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.state.mode = InteractionMode::Edit {
        thought_id: original_id,
    };
    app.sync_editor_from_state();
    app.queue_screenshot_candidates([candidate(15)]);
    let capture = next_commit(&mut app, &mut ids, &clock);
    assert!(
        app.handle(UiInput::Key(UiKey::Character('!')), &mut ids, &clock)
            .is_empty()
    );
    assert!(
        app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock)
            .is_empty()
    );
    assert!(!app.quit);

    let effects = app.complete_screenshot_capture(Ok(created(&capture)), &mut ids, &clock);
    assert!(app.quit);
    let [Effect::CommitRevision(revision)] = effects.as_slice() else {
        panic!("deferred editor revision");
    };
    assert_eq!(revision.after_content, "active!");
}

#[test]
fn capture_failure_replays_quit_as_truthful_ready_confirmation() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(18)]);
    next_commit(&mut app, &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock);

    let effects = app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    assert_eq!(effects, vec![Effect::Screenshot(ScreenshotIntent::Disable)]);
    assert!(!app.quit);
    assert!(app.screenshot_retry_ready());
    let retry = app.retry_screenshot_capture(&mut ids, &clock);
    let [Effect::CommitCapture(retry)] = retry.as_slice() else {
        panic!("explicit retry");
    };
    app.complete_screenshot_capture(Ok(created(retry)), &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock);
    assert!(app.quit);
    assert_eq!(app.state.board.live_thoughts().len(), 2);
}

#[test]
fn explicit_editor_interaction_prevents_burst_auto_advance() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(16), candidate(17)]);
    let first = next_commit(&mut app, &mut ids, &clock);
    let first_id = capture_thought_id(&first);
    app.complete_screenshot_capture(Ok(created(&first)), &mut ids, &clock);

    app.handle(UiInput::Key(UiKey::Character('x')), &mut ids, &clock);
    let revision_effects = app.handle(
        UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: true,
        }),
        &mut ids,
        &clock,
    );
    let editor_before = app.editor_snapshot().expect("interacted editor");
    let [Effect::CommitRevision(revision)] = revision_effects.as_slice() else {
        panic!("editor revision");
    };
    app.acknowledge_persistence_result(revision.sequence, Ok(()));
    let second = next_commit(&mut app, &mut ids, &clock);

    app.complete_screenshot_capture(Ok(created(&second)), &mut ids, &clock);
    assert_eq!(app.state.focused_thought, Some(first_id));
    assert_eq!(
        app.state.mode,
        InteractionMode::Edit {
            thought_id: first_id
        }
    );
    assert_eq!(app.editor_snapshot(), Some(editor_before));
    assert_eq!(
        app.state
            .board
            .thought(first_id)
            .expect("first capture")
            .content,
        format!("{}x", candidate(16).path.to_string_lossy())
    );
    assert_eq!(app.state.board.live_thoughts().len(), 3);
}

#[test]
fn passive_pointer_focus_and_resize_keep_auto_ready_but_a_click_invalidates_it() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(20), candidate(21)]);
    let first = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Ok(created(&first)), &mut ids, &clock);
    assert!(app.screenshot.auto_ready.is_some());

    for input in [
        UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Move,
            extend_selection: false,
        }),
        UiInput::HostFocusGained,
        UiInput::Resize {
            width: 40,
            height: 8,
        },
    ] {
        app.handle(input, &mut ids, &clock);
        assert!(app.screenshot.auto_ready.is_some());
    }

    app.handle(
        UiInput::Pointer(PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert!(app.screenshot.auto_ready.is_none());
}

#[test]
fn palette_names_are_exact_in_both_states() {
    let (mut app, _, _, _) = app_with_thought();
    app.open_palette();
    let (_, entries, _) = app.palette_view().expect("palette");
    assert!(
        entries
            .iter()
            .any(|entry| entry == "Enable Screenshot Inbox")
    );
    app.close_overlay();

    app.screenshot_started(Duration::ZERO);
    app.open_palette();
    let (_, entries, _) = app.palette_view().expect("palette");
    assert!(
        entries
            .iter()
            .any(|entry| entry == "Disable Screenshot Inbox")
    );
}

#[test]
fn takeover_keyboard_and_mouse_choices_emit_verified_owner_requests() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    let session_id = app.state.board.session.id;
    let keyboard_owner = owner(&mut ids, session_id);
    app.screenshot_conflict(keyboard_owner.clone());
    assert!(
        app.handle(
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        )
        .is_empty()
    );
    let effects = app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Screenshot(ScreenshotIntent::TakeOver { owner: requested, .. })]
            if requested == &keyboard_owner
    ));

    app.screenshot_failed(&crate::ports::screenshot::ScreenshotError::TakeoverFailed);
    let mouse_owner = owner(&mut ids, session_id);
    app.screenshot_conflict(mouse_owner.clone());
    let layout = app.prepare_frame(Rect::new(0, 0, 60, 12));
    let take_over = layout.overlay.expect("takeover overlay").items[1];
    let effects = app.handle(
        UiInput::Pointer(PointerInput {
            column: take_over.x,
            row: take_over.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        }),
        &mut ids,
        &clock,
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::Screenshot(ScreenshotIntent::TakeOver { owner: requested, .. })]
            if requested == &mouse_owner
    ));
}

pub(super) fn next_commit(
    app: &mut BoardApp,
    ids: &mut FakeIdGenerator,
    clock: &FakeClock,
) -> CaptureCommit {
    let effects = app.advance_screenshot_capture(ids, clock);
    let [Effect::CommitCapture(commit)] = effects.as_slice() else {
        panic!("capture commit");
    };
    commit.clone()
}

pub(super) fn app_with_thought() -> (
    BoardApp,
    FakeIdGenerator,
    FakeClock,
    crate::domain::ThoughtId,
) {
    let mut ids = FakeIdGenerator::new(1_725_260_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("screenshot-ui"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session.id,
        "active".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    (
        BoardApp::new(AppState::new(board), RopeEditorFactory),
        ids,
        FakeClock::new(Timestamp::from_millis(2)),
        thought_id,
    )
}

pub(super) fn candidate(byte: u8) -> ScreenshotCandidate {
    ScreenshotCandidate {
        fingerprint: ScreenshotFingerprint([byte; 32]),
        path: std::env::temp_dir().join(format!("Unicode capture {byte} 🖼️.png")),
        image_type: ScreenshotImageType::Png,
    }
}

fn capture_thought_id(commit: &CaptureCommit) -> crate::domain::ThoughtId {
    match &commit.operation.forward {
        crate::domain::BoardMutation::AddThought { thought } => thought.id,
        _ => panic!("add thought"),
    }
}

pub(super) fn created(commit: &CaptureCommit) -> CaptureCommitOutcome {
    let thought_id = capture_thought_id(commit);
    CaptureCommitOutcome::Created {
        durable: CommitReceipt {
            session_id: commit.operation.session_id,
            sequence: commit.operation.sequence,
            identity: DurableIdentity::Operation(commit.operation.id),
            idempotent_replay: false,
        },
        capture: CaptureReceipt {
            source: commit.source,
            session_id: commit.operation.session_id,
            thought_id,
            operation_id: commit.operation.id,
            accepted_at: commit.operation.created_at,
        },
    }
}

fn owner(ids: &mut FakeIdGenerator, session_id: crate::domain::SessionId) -> CaptureOwnerInfo {
    CaptureOwnerInfo {
        instance_id: ids.instance_id(),
        session_id,
        pid: 42,
        version: "test".to_owned(),
        capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
        control_protocol: crate::ports::control::CONTROL_PROTOCOL_VERSION,
        control_endpoint: "private-control-endpoint".to_owned(),
        started_at: Timestamp::from_millis(1),
    }
}
