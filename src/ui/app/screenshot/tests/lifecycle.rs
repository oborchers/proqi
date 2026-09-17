use super::behavior::{app_with_thought, candidate, next_commit};
use crate::{
    application::{DurabilityState, Effect, FailureCode, ScreenshotIntent, ScreenshotPauseReason},
    domain::{OperationSequence, Timestamp},
    ports::store::StoreError,
    ui::{PointerButton, PointerInput, PointerKind, ScreenshotUpdateReadiness, UiInput, UiKey},
};
use ratatui_core::layout::Rect;

#[test]
fn commands_relevance_tracks_inactive_active_paused_and_failed_capture_states() {
    let (mut app, _ids, _clock, _) = app_with_thought();
    app.open_palette();
    let (_, inactive, _) = app.palette_view().expect("inactive Commands");
    assert!(!inactive.contains(&"Enable Screenshot Inbox".to_owned()));

    app.close_overlay();
    app.screenshot_started(std::time::Duration::ZERO);
    app.open_palette();
    let (_, active, _) = app.palette_view().expect("active Commands");
    assert!(active.contains(&"Disable Screenshot Inbox".to_owned()));

    app.close_overlay();
    app.enter_screenshot_paused(ScreenshotPauseReason::Inactivity { minutes: 20 });
    app.open_palette();
    let (_, paused, _) = app.palette_view().expect("paused Commands");
    assert!(paused.contains(&"Resume Screenshot Inbox".to_owned()));

    let (mut failed_app, mut failed_ids, failed_clock, _) = app_with_thought();
    failed_app.screenshot_started(std::time::Duration::ZERO);
    failed_app.queue_screenshot_candidates([candidate(50)]);
    next_commit(&mut failed_app, &mut failed_ids, &failed_clock);
    failed_app.complete_screenshot_capture(Err(StoreError::Busy), &mut failed_ids, &failed_clock);
    failed_app.open_palette();
    let (_, failed, _) = failed_app.palette_view().expect("failed Commands");
    assert!(failed.contains(&"Retry Screenshot Capture".to_owned()));
}

#[test]
fn open_commands_learns_that_an_in_flight_capture_became_retryable() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(59)]);
    next_commit(&mut app, &mut ids, &clock);
    app.open_palette();
    let (_, before, selected) = app.palette_view().expect("Commands during capture");
    assert_eq!(before[selected], "New thought");
    assert!(!before.contains(&"Retry Screenshot Capture".to_owned()));

    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);

    let (_, after, selected) = app.palette_view().expect("Commands after capture failure");
    assert_eq!(after[selected], "New thought");
    assert!(after.contains(&"Retry Screenshot Capture".to_owned()));
}

#[test]
fn update_readiness_blocks_live_queued_and_retryable_capture_state() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    assert_eq!(
        app.screenshot_update_readiness(),
        ScreenshotUpdateReadiness::Ready
    );

    app.screenshot_started(std::time::Duration::ZERO);
    assert_eq!(
        app.screenshot_update_readiness(),
        ScreenshotUpdateReadiness::Blocked
    );
    app.queue_screenshot_candidates([candidate(50)]);
    next_commit(&mut app, &mut ids, &clock);
    assert_eq!(
        app.screenshot_update_readiness(),
        ScreenshotUpdateReadiness::CommitInFlight
    );
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    assert_eq!(
        app.screenshot_update_readiness(),
        ScreenshotUpdateReadiness::Blocked
    );
}

#[test]
fn disable_and_retry_are_distinct_truthful_public_actions() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(51)]);
    next_commit(&mut app, &mut ids, &clock);
    assert_eq!(
        app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );

    app.open_palette();
    let (_, commands, _) = app.palette_view().expect("palette");
    assert!(!commands.iter().any(
        |command| command.contains("Screenshot Inbox") && command != "Retry Screenshot Capture"
    ));
    assert!(
        commands
            .iter()
            .any(|command| command == "Retry Screenshot Capture")
    );
    app.close_overlay();
    assert!(app.toggle_screenshot_inbox(&mut ids, &clock).is_empty());
    assert!(app.screenshot_retry_ready());
    assert!(matches!(
        app.retry_screenshot_capture(&mut ids, &clock).as_slice(),
        [Effect::CommitCapture(_)]
    ));
}

#[test]
fn failed_durability_disables_retained_capture_retry_in_commands_and_execution() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(73)]);
    next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    assert!(app.screenshot_retry_ready());
    app.state.durability = DurabilityState::Failed {
        durable: OperationSequence::ZERO,
        failed: OperationSequence::new(1),
        code: FailureCode::StorageFailed,
    };

    app.open_palette();
    for character in "retry screenshot capture".chars() {
        app.handle(UiInput::Key(UiKey::Character(character)), &mut ids, &clock);
    }
    let picker = app.command_palette_view().expect("Commands");
    assert_eq!(picker.rows.len(), 1);
    assert!(!picker.rows[0].enabled);
    assert_eq!(
        picker.rows[0].secondary.as_deref(),
        Some("Resolve the failed save first")
    );
    let thought_count = app.state.board.live_thoughts().len();
    assert!(
        app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock)
            .is_empty()
    );
    assert_eq!(app.state.board.live_thoughts().len(), thought_count);
    app.close_overlay();
    assert!(app.retry_screenshot_capture(&mut ids, &clock).is_empty());
    assert_eq!(app.state.board.live_thoughts().len(), thought_count);

    let (mut paused, mut paused_ids, paused_clock, _) = app_with_thought();
    paused.enter_screenshot_paused(ScreenshotPauseReason::Inactivity { minutes: 20 });
    paused.state.durability = DurabilityState::Failed {
        durable: OperationSequence::ZERO,
        failed: OperationSequence::new(1),
        code: FailureCode::StorageFailed,
    };
    paused.open_palette();
    for character in "resume screenshot inbox".chars() {
        paused.handle(
            UiInput::Key(UiKey::Character(character)),
            &mut paused_ids,
            &paused_clock,
        );
    }
    let picker = paused.command_palette_view().expect("Commands");
    assert_eq!(picker.rows.len(), 1);
    assert!(!picker.rows[0].enabled);
    assert_eq!(
        picker.rows[0].secondary.as_deref(),
        Some("Resolve the failed save first")
    );
    paused.close_overlay();
    assert!(
        paused
            .toggle_screenshot_inbox(&mut paused_ids, &paused_clock)
            .is_empty()
    );
}

#[test]
fn stopped_state_remains_releasing_until_authority_is_released() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(58)]);
    next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    app.screenshot_stopping_completed();

    assert!(app.toggle_screenshot_inbox(&mut ids, &clock).is_empty());
    assert!(app.screenshot_retry_ready());
    assert!(app.status_text().is_some_and(|text| text.contains("Retry")));

    app.screenshot_authority_released();
    assert_eq!(
        app.toggle_screenshot_inbox(&mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Enable)]
    );
}

#[test]
fn ready_quit_is_bounded_explicit_and_never_silently_discards() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(52)]);
    next_commit(&mut app, &mut ids, &clock);
    assert_eq!(
        app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );

    assert!(
        app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock)
            .is_empty()
    );
    assert!(!app.quit);
    assert!(app.screenshot_retry_ready());
    assert!(
        app.status_text()
            .is_some_and(|status| status.contains("quit again to abandon"))
    );
    assert!(
        app.handle(
            UiInput::Pointer(PointerInput {
                column: 0,
                row: 0,
                kind: PointerKind::Move,
                extend_selection: false,
            }),
            &mut ids,
            &clock,
        )
        .is_empty()
    );
    assert!(!app.quit);
    assert!(app.screenshot_retry_ready());
    assert!(
        app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock)
            .is_empty()
    );
    assert!(app.quit);
    assert!(!app.screenshot_retry_ready());
}

#[test]
fn commands_quit_keeps_confirmation_open_for_a_second_enter() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(74)]);
    next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    app.open_palette();
    for character in "quit proqi".chars() {
        app.handle(UiInput::Key(UiKey::Character(character)), &mut ids, &clock);
    }
    app.prepare_frame(Rect::new(0, 0, 60, 12));

    assert!(
        app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock)
            .is_empty()
    );
    assert!(!app.quit);
    assert!(app.screenshot_retry_ready());
    assert!(app.command_palette_view().is_some());
    assert!(
        app.status_text()
            .is_some_and(|status| status.contains("quit again to abandon"))
    );

    app.prepare_frame(Rect::new(0, 0, 60, 12));
    assert!(
        app.handle(UiInput::Key(UiKey::Enter), &mut ids, &clock)
            .is_empty()
    );
    assert!(app.quit);
    assert!(!app.screenshot_retry_ready());
    assert!(app.command_palette_view().is_none());
}

#[test]
fn commands_quit_pointer_uses_current_row_geometry_for_both_confirmations() {
    let (mut app, mut ids, mut clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(75)]);
    next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);
    app.open_palette();
    for character in "quit proqi".chars() {
        app.handle(UiInput::Key(UiKey::Character(character)), &mut ids, &clock);
    }
    let first = app
        .prepare_frame(Rect::new(0, 0, 60, 12))
        .overlay
        .expect("Commands overlay")
        .items[0];
    let pointer_down = |area: ratatui_core::layout::Rect| {
        UiInput::Pointer(PointerInput {
            column: area.x,
            row: area.y,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        })
    };

    assert!(app.handle(pointer_down(first), &mut ids, &clock).is_empty());
    assert!(!app.quit);
    assert!(app.screenshot_retry_ready());
    assert!(app.command_palette_view().is_some());
    clock.set(Timestamp::from_millis(1_000));

    let second = app
        .prepare_frame(Rect::new(0, 0, 60, 12))
        .overlay
        .expect("retained Commands overlay")
        .items[0];
    assert!(
        app.handle(pointer_down(second), &mut ids, &clock)
            .is_empty()
    );
    assert!(app.quit);
    assert!(!app.screenshot_retry_ready());
    assert!(app.command_palette_view().is_none());
}

#[test]
fn shutdown_retains_candidates_that_crossed_the_watcher_acceptance_boundary() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock);
    assert!(app.quit);
    assert!(app.queue_screenshot_candidates([candidate(53)]).is_empty());
    assert_eq!(app.screenshot.candidates.len(), 1);
    assert!(matches!(
        app.advance_screenshot_capture(&mut ids, &clock).as_slice(),
        [Effect::CommitCapture(_)]
    ));
    assert!(app.screenshot_sequence_reserved());
}

#[test]
fn deferred_quit_drains_the_next_emitted_and_final_reconcile_candidates() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(54)]);
    let first = next_commit(&mut app, &mut ids, &clock);
    app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock);
    app.queue_screenshot_candidates([candidate(55)]);
    app.complete_screenshot_capture(Ok(super::behavior::created(&first)), &mut ids, &clock);
    assert!(app.quit);
    let second_effects = app.advance_screenshot_capture(&mut ids, &clock);
    let [Effect::CommitCapture(second)] = second_effects.as_slice() else {
        panic!("second accepted candidate");
    };
    assert_eq!(second.source, candidate(55).fingerprint);

    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    assert_eq!(
        app.toggle_screenshot_inbox(&mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    app.queue_screenshot_candidates([candidate(56)]);
    app.screenshot_stopping_completed();
    app.queue_screenshot_candidates([candidate(57)]);
    assert_eq!(app.screenshot.candidates.len(), 2);
}
