//! Capture into the invoking tab's session: source precedence, the shared
//! session rule, the tab record, feedback, and the guarantee that a failure
//! stores and creates nothing.

use std::path::PathBuf;

use crate::{
    adapters::memory::ScriptedClipboard,
    application::{CaptureError, CaptureSource},
    ports::{
        clipboard::{ClipboardContent, ClipboardError},
        companion::{CompanionContext, CompanionSessionState},
    },
};

use super::{
    super::{CompanionCaptureError, capture_to_companion},
    OTHER, OWN,
    fakes::{
        FakeHost, FakeRecords, FakeSessions, UnwritableRecords, agent, context, record, session,
    },
};

fn host_with_selection(selection: Option<&str>) -> FakeHost {
    let context = CompanionContext {
        selected_text: selection.map(str::to_owned),
        ..context("w1:p1")
    };
    FakeHost::new("w1:p1", vec![agent("w1:p1", true)]).with_context(context)
}

#[test]
fn the_selection_is_stored_exactly_in_the_tab_session_without_reading_the_clipboard() {
    let mut host = host_with_selection(Some("  exact\r\nselection\t"));
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let mut clipboard = ScriptedClipboard::text("clipboard");
    let outcome = capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
        .expect("capture");
    assert_eq!(outcome.source, CaptureSource::Selection);
    assert_eq!(outcome.session_id, session(OWN));
    assert_eq!(outcome.characters, 18);
    assert_eq!(clipboard.reads, 0);
    assert_eq!(
        sessions.captured,
        vec![(session(OWN), "  exact\r\nselection\t".to_owned())]
    );
    assert_eq!(
        sessions.ensured,
        vec![("agent-tab".to_owned(), PathBuf::from("/work"))]
    );
    assert_eq!(
        host.notifications,
        vec!["Captured to Proqi: \"exact selection\" (18 characters)".to_owned()]
    );
    // The tab keeps the session it captured into, without claiming a pane.
    assert_eq!(records.0, vec![record(None, OWN)]);
    assert!(host.calls.is_empty(), "capture opens and focuses nothing");
}

#[test]
fn without_a_selection_the_clipboard_text_is_stored() {
    let mut host = host_with_selection(None);
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let mut clipboard = ScriptedClipboard::text("from the clipboard\n");
    let outcome = capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
        .expect("capture");
    assert_eq!(outcome.source, CaptureSource::Clipboard);
    assert_eq!(clipboard.reads, 1);
    assert_eq!(
        sessions.captured,
        vec![(session(OWN), "from the clipboard\n".to_owned())]
    );
}

#[test]
fn a_recorded_session_is_reused_even_when_its_owner_is_live() {
    let mut host = host_with_selection(Some("note"));
    let mut records = FakeRecords(vec![record(Some("w1:p9"), OTHER)]);
    let mut sessions = FakeSessions::default().with_state(OTHER, CompanionSessionState::Active);
    let mut clipboard = ScriptedClipboard::text("unused");
    capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard).expect("capture");
    assert_eq!(sessions.captured, vec![(session(OTHER), "note".to_owned())]);
    assert!(sessions.ensured.is_empty());
    assert_eq!(host.agent_queries, 0);
    assert_eq!(
        records.0,
        vec![record(Some("w1:p9"), OTHER)],
        "record unchanged"
    );
}

#[test]
fn a_trashed_recorded_session_falls_back_to_the_named_rule_like_the_toggle() {
    let mut host = host_with_selection(Some("note")).with_agent_names(&["api-claude"]);
    let mut records = FakeRecords(vec![record(None, OTHER)]);
    let mut sessions = FakeSessions::with_named("api-claude", OWN)
        .with_state(OTHER, CompanionSessionState::Unavailable);
    let mut clipboard = ScriptedClipboard::text("unused");
    capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard).expect("capture");
    assert_eq!(sessions.captured, vec![(session(OWN), "note".to_owned())]);
    assert_eq!(
        records.0,
        vec![record(None, OTHER)],
        "toggle owns re-recording"
    );
}

#[test]
fn empty_non_text_and_oversized_input_create_and_store_nothing() {
    let cases = [
        (
            Some(" \n"),
            ScriptedClipboard::text("unused"),
            "Nothing captured: the selection is empty",
        ),
        (
            None,
            ScriptedClipboard::text(""),
            "Nothing captured: the clipboard is empty",
        ),
        (
            None,
            ScriptedClipboard::with(Err(ClipboardError::InvalidImage)),
            "Nothing captured: the clipboard holds no text (for example an image)",
        ),
        (
            None,
            ScriptedClipboard::with(Ok(ClipboardContent::Text(
                crate::ports::clipboard::ClipboardText::plain("x".repeat(128 * 1024 + 1)),
            ))),
            "Nothing captured: the clipboard has 131073 bytes, more than the 131072-byte thought limit",
        ),
    ];
    for (selection, mut clipboard, message) in cases {
        let mut host = host_with_selection(selection);
        let mut records = FakeRecords::default();
        let mut sessions = FakeSessions::with_named("agent-tab", OWN);
        let error = capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
            .expect_err("rejected");
        assert!(matches!(error, CompanionCaptureError::Capture(_)));
        assert_eq!(host.notifications, vec![message.to_owned()]);
        assert!(sessions.ensured.is_empty(), "no session is created");
        assert!(sessions.captured.is_empty());
        assert!(records.0.is_empty());
        assert_eq!(host.agent_queries, 0);
    }
}

#[test]
fn an_unreadable_clipboard_keeps_its_cause_and_stores_nothing() {
    let mut host = host_with_selection(None);
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let mut clipboard = ScriptedClipboard::with(Err(ClipboardError::TimedOut));
    let error = capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
        .expect_err("unreadable");
    assert!(matches!(
        error,
        CompanionCaptureError::Capture(CaptureError::Clipboard(ClipboardError::TimedOut))
    ));
    assert!(sessions.captured.is_empty());
}

#[test]
fn a_failed_agent_query_stores_nothing_instead_of_guessing_a_session() {
    let mut host = host_with_selection(Some("note"));
    host.agent_names = None;
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let mut clipboard = ScriptedClipboard::text("unused");
    let error = capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
        .expect_err("host");
    assert!(matches!(error, CompanionCaptureError::Host(_)));
    assert!(sessions.ensured.is_empty());
    assert!(sessions.captured.is_empty());
    assert_eq!(
        host.notifications,
        vec!["Nothing captured: agent list timed out".to_owned()]
    );
}

#[test]
fn a_store_failure_is_reported_and_leaves_no_record() {
    let mut host = host_with_selection(Some("note"));
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    sessions.fail_capture = Some("session is busy".to_owned());
    let mut clipboard = ScriptedClipboard::text("unused");
    let error = capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
        .expect_err("store");
    assert!(matches!(error, CompanionCaptureError::Session(_)));
    assert!(records.0.is_empty());
    assert_eq!(
        host.notifications,
        vec!["Nothing captured: session is busy".to_owned()]
    );
}

#[test]
fn an_unwritable_record_does_not_undo_a_durable_capture() {
    let mut host = host_with_selection(Some("note"));
    let mut records = UnwritableRecords(Vec::new());
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    let mut clipboard = ScriptedClipboard::text("unused");
    capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard).expect("capture");
    assert_eq!(sessions.captured.len(), 1);
    assert_eq!(
        host.notifications,
        vec!["Captured to Proqi: \"note\" (4 characters)".to_owned()]
    );
}

#[test]
fn repeated_captures_of_the_same_text_are_each_stored() {
    let mut records = FakeRecords::default();
    let mut sessions = FakeSessions::with_named("agent-tab", OWN);
    for _ in 0..3 {
        let mut host = host_with_selection(Some("same"));
        let mut clipboard = ScriptedClipboard::text("unused");
        capture_to_companion(&mut host, &mut records, &mut sessions, &mut clipboard)
            .expect("capture");
    }
    assert_eq!(sessions.captured.len(), 3, "no silent deduplication");
    assert_eq!(sessions.ensured.len(), 1, "later captures reuse the record");
}
