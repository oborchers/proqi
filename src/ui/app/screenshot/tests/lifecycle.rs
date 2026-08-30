use super::behavior::{app_with_thought, candidate, next_commit};
use crate::{
    application::{Effect, ScreenshotIntent},
    ports::store::StoreError,
    ui::{UiInput, UiKey},
};

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
        app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock)
            .is_empty()
    );
    assert!(app.quit);
    assert!(!app.screenshot_retry_ready());
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
