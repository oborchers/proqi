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
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);

    app.open_palette();
    let (_, commands, _) = app.palette_view().expect("palette");
    assert!(
        commands
            .iter()
            .any(|command| command == "Disable Screenshot Inbox")
    );
    assert!(
        commands
            .iter()
            .any(|command| command == "Retry Screenshot Capture")
    );
    app.close_overlay();
    assert_eq!(
        app.toggle_screenshot_inbox(&mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    );
    assert!(app.screenshot_retry_ready());
    assert!(matches!(
        app.retry_screenshot_capture(&mut ids, &clock).as_slice(),
        [Effect::CommitCapture(_)]
    ));
}

#[test]
fn ready_quit_is_bounded_explicit_and_never_silently_discards() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.queue_screenshot_candidates([candidate(52)]);
    next_commit(&mut app, &mut ids, &clock);
    app.complete_screenshot_capture(Err(StoreError::Busy), &mut ids, &clock);

    assert_eq!(
        app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock),
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
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
fn shutdown_admits_neither_late_candidates_nor_a_new_capture_commit() {
    let (mut app, mut ids, clock, _) = app_with_thought();
    app.screenshot_started(std::time::Duration::ZERO);
    app.handle(UiInput::Key(UiKey::Quit), &mut ids, &clock);
    assert!(app.quit);
    assert!(app.queue_screenshot_candidates([candidate(53)]).is_empty());
    assert!(app.screenshot.candidates.is_empty());
    assert!(app.advance_screenshot_capture(&mut ids, &clock).is_empty());
    assert!(!app.screenshot_sequence_reserved());
}
