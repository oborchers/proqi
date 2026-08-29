use std::time::Duration;

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, ScreenshotIntent, ScreenshotPauseReason},
    domain::{Session, SessionBoard, Timestamp},
    ports::{
        environment::IdGenerator as _,
        screenshot::{
            ScreenshotActivityPolicy, ScreenshotCandidate, ScreenshotFingerprint,
            ScreenshotImageType,
        },
        store::{
            CaptureCommit, CaptureCommitOutcome, CaptureReceipt, CommitReceipt, DurableIdentity,
            StoreError,
        },
    },
    ui::{BoardApp, UiInput, UiKey},
};

#[test]
fn a_burst_admits_only_the_hard_limit_and_resumes_from_a_fresh_lease() {
    let (mut app, mut ids, clock) = app();
    configure(&mut app, 1, 2);
    app.screenshot_started(Duration::ZERO);

    assert_eq!(
        app.queue_screenshot_candidates([candidate(1), candidate(2), candidate(3)]),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    assert_eq!(
        app.screenshot_stopped(),
        vec![Effect::NotifyScreenshotPause(
            ScreenshotPauseReason::CaptureLimit { captures: 2 }
        )]
    );
    assert_eq!(
        app.status_text(),
        Some("Screenshot Inbox paused after 2 unattended captures")
    );
    assert!(app.screenshot_stopped().is_empty());
    assert_eq!(
        app.status_text(),
        Some("Screenshot Inbox paused after 2 unattended captures")
    );

    for expected in [1, 2] {
        let commit = next_commit(&mut app, &mut ids, &clock);
        assert_eq!(commit.source, candidate(expected).fingerprint);
        app.complete_screenshot_capture(Ok(created(&commit)), &mut ids, &clock);
    }
    assert!(app.advance_screenshot_capture(&mut ids, &clock).is_empty());
    assert_eq!(app.state.board.live_thoughts().len(), 2);
    assert_eq!(
        app.toggle_screenshot_inbox(&mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Enable)]
    );
}

#[test]
fn deliberate_input_renews_time_and_count_but_focus_and_resize_do_not() {
    let (mut app, _, _) = app();
    configure(&mut app, 1, 2);
    app.screenshot_started(Duration::ZERO);
    assert!(app.queue_screenshot_candidates([candidate(4)]).is_empty());
    app.note_screenshot_activity(
        &UiInput::Key(UiKey::Character('x')),
        Duration::from_secs(30),
    );
    assert!(app.queue_screenshot_candidates([candidate(5)]).is_empty());
    app.note_screenshot_activity(&UiInput::HostFocusGained, Duration::from_secs(59));
    app.note_screenshot_activity(
        &UiInput::Resize {
            width: 40,
            height: 8,
        },
        Duration::from_secs(59),
    );
    assert!(
        app.advance_screenshot_activity(Duration::from_secs(89))
            .is_empty()
    );
    assert_eq!(
        app.advance_screenshot_activity(Duration::from_secs(90)),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
}

#[test]
fn auto_pause_survives_final_reconcile_failure_and_is_resumable() {
    let (mut app, mut ids, clock) = app();
    configure(&mut app, 1, 10);
    app.screenshot_started(Duration::ZERO);
    assert_eq!(
        app.advance_screenshot_activity(Duration::from_secs(60)),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    assert_eq!(
        app.screenshot_failed(&crate::ports::screenshot::ScreenshotError::Reconciliation),
        vec![Effect::NotifyScreenshotPause(
            ScreenshotPauseReason::Inactivity { minutes: 1 }
        )]
    );
    assert!(
        app.status_text()
            .is_some_and(|status| status.contains("final reconciliation failed"))
    );
    app.handle(UiInput::Key(UiKey::Escape), &mut ids, &clock);
    assert_eq!(
        app.status_text(),
        Some("Screenshot Inbox paused after 1 minute without activity")
    );
    app.open_palette();
    let (_, entries, _) = app.palette_view().expect("palette");
    assert!(
        entries
            .iter()
            .any(|entry| entry == "Resume Screenshot Inbox")
    );
}

#[test]
fn cap_failure_retains_ordered_retry_without_admitting_excess_work() {
    let (mut app, mut ids, clock) = app();
    configure(&mut app, 1, 2);
    app.screenshot_started(Duration::ZERO);
    app.queue_screenshot_candidates([candidate(6), candidate(7), candidate(8)]);
    app.screenshot_stopped();

    let first = next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    let retry_effects = app.toggle_screenshot_inbox(&mut ids, &clock);
    let [Effect::CommitCapture(retry)] = retry_effects.as_slice() else {
        panic!("retained retry");
    };
    assert_eq!(retry.source, first.source);
    let retry = retry.clone();
    app.complete_screenshot_capture(Ok(created(&retry)), &mut ids, &clock);
    let second = next_commit(&mut app, &mut ids, &clock);
    assert_eq!(second.source, candidate(7).fingerprint);
    app.complete_screenshot_capture(Ok(created(&second)), &mut ids, &clock);
    assert!(app.advance_screenshot_capture(&mut ids, &clock).is_empty());
    assert_eq!(app.state.board.live_thoughts().len(), 2);
}

fn configure(app: &mut BoardApp, minutes: u16, captures: u16) {
    app.configure_screenshot_activity(
        ScreenshotActivityPolicy::new(minutes, captures).expect("valid policy"),
    );
}

fn app() -> (BoardApp, FakeIdGenerator, FakeClock) {
    let mut ids = FakeIdGenerator::new(1_725_280_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("screenshot-activity"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    (
        BoardApp::new(
            AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
            RopeEditorFactory,
        ),
        ids,
        FakeClock::new(Timestamp::from_millis(2)),
    )
}

fn candidate(byte: u8) -> ScreenshotCandidate {
    ScreenshotCandidate {
        fingerprint: ScreenshotFingerprint([byte; 32]),
        path: std::env::temp_dir().join(format!("capture-{byte}.png")),
        image_type: ScreenshotImageType::Png,
    }
}

fn next_commit(app: &mut BoardApp, ids: &mut FakeIdGenerator, clock: &FakeClock) -> CaptureCommit {
    let effects = app.advance_screenshot_capture(ids, clock);
    let [Effect::CommitCapture(commit)] = effects.as_slice() else {
        panic!("capture commit");
    };
    commit.clone()
}

fn created(commit: &CaptureCommit) -> CaptureCommitOutcome {
    let crate::domain::BoardMutation::AddThought { thought } = &commit.operation.forward else {
        panic!("add thought");
    };
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
            thought_id: thought.id,
            operation_id: commit.operation.id,
            accepted_at: commit.operation.created_at,
        },
    }
}
