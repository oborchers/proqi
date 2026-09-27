//! Forwarded explicit captures append quietly and share the new-capture status.

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, InteractionMode},
    domain::{
        OperationSequence, Session, SessionBoard, Thought, ThoughtId, ThoughtPosition, Timestamp,
    },
    ports::{
        control::{AddAnnouncement, ControlMutation},
        environment::IdGenerator as _,
    },
};

use super::super::BoardApp;

fn editing_app() -> (BoardApp, FakeIdGenerator, ThoughtId) {
    let mut ids = FakeIdGenerator::new(1_725_270_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-capture"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let first = ids.thought_id();
    let thoughts = ["first draft", "second"]
        .iter()
        .enumerate()
        .map(|(index, content)| {
            let id = if index == 0 { first } else { ids.thought_id() };
            Thought::new(
                id,
                session.id,
                (*content).to_owned(),
                ThoughtPosition::new(u32::try_from(index).expect("position")),
                Timestamp::from_millis(1),
            )
        })
        .collect();
    let board = SessionBoard::new(session, thoughts).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    app.state.mode = InteractionMode::Edit { thought_id: first };
    app.state.insertion_index = 1;
    app.sync_editor_from_state();
    app.help = true;
    (app, ids, first)
}

fn add(
    ids: &mut FakeIdGenerator,
    content: &str,
    announcement: Option<AddAnnouncement>,
) -> (ControlMutation, ThoughtId) {
    let operation_id = ids.operation_id();
    let thought_id = ThoughtId::from_database_bytes(operation_id.database_bytes()).expect("id");
    (
        ControlMutation::Add {
            operation_id,
            thought_id,
            content: content.to_owned(),
            annotations: Vec::new(),
            position: None,
            announcement,
        },
        thought_id,
    )
}

fn sequence(effects: &[Effect]) -> OperationSequence {
    effects
        .iter()
        .find_map(Effect::persistence_batch)
        .and_then(|batch| batch.sequence())
        .expect("persistence sequence")
}

#[test]
fn a_capture_appends_at_the_end_without_moving_focus_caret_or_overlays() {
    let (mut app, mut ids, first) = editing_app();
    let editor_before = app.editor_snapshot().expect("editor");
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let (mutation, captured) = add(
        &mut ids,
        "  exact\r\ncapture ",
        Some(AddAnnouncement::Capture),
    );
    let effects = app.handle_control(&mutation, &clock).expect("capture");

    let live = app.state.board.live_thoughts();
    assert_eq!(live.len(), 3);
    assert_eq!(live[2].id, captured, "appended after every thought");
    assert_eq!(live[2].content, "  exact\r\ncapture ");
    assert_eq!(app.state.mode, InteractionMode::Edit { thought_id: first });
    assert_eq!(app.state.focused_thought_id(), Some(first));
    assert_eq!(app.editor_snapshot().expect("editor"), editor_before);
    assert!(app.help, "an open overlay stays open");
    assert_eq!(
        app.status_text(),
        None,
        "nothing is announced before durability"
    );

    app.acknowledge_persistence_result(sequence(&effects), Ok(()));
    assert_eq!(app.status_text(), Some("1 new capture"));
    assert_eq!(app.state.mode, InteractionMode::Edit { thought_id: first });

    let (mutation, _) = add(&mut ids, "again", Some(AddAnnouncement::Capture));
    let effects = app.handle_control(&mutation, &clock).expect("capture");
    app.acknowledge_persistence_result(sequence(&effects), Ok(()));
    assert_eq!(app.status_text(), Some("2 new captures"));
    assert_eq!(app.editor_snapshot().expect("editor"), editor_before);
}

#[test]
fn a_failed_capture_save_is_never_announced() {
    let (mut app, mut ids, _) = editing_app();
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let (mutation, _) = add(&mut ids, "lost", Some(AddAnnouncement::Capture));
    let effects = app.handle_control(&mutation, &clock).expect("capture");
    app.acknowledge_persistence_result(
        sequence(&effects),
        Err(crate::application::FailureCode::StorageFailed),
    );
    assert_ne!(app.status_text(), Some("1 new capture"));
    assert_eq!(app.screenshot.notice_count, 0);
}

#[test]
fn an_ordinary_add_keeps_its_position_and_is_not_announced() {
    let (mut app, mut ids, _) = editing_app();
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let (mutation, added) = add(&mut ids, "api", None);
    let effects = app.handle_control(&mutation, &clock).expect("add");
    assert_eq!(
        app.state.board.live_thoughts()[1].id,
        added,
        "insertion point"
    );
    app.acknowledge_persistence_result(sequence(&effects), Ok(()));
    assert_eq!(app.status_text(), None);
}
