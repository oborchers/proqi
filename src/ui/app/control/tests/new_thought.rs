//! New-thought behavior while owner control preserves transient Compose.

use crate::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, InteractionMode},
    domain::{Session, SessionBoard, Timestamp},
    ports::{control::ControlMutation, environment::IdGenerator},
    ui::{PointerButton, PointerInput, PointerKind},
};

use super::super::BoardApp;

#[test]
fn prompt_click_after_an_owner_add_only_engages_the_preserved_compose_editor() {
    let mut ids = FakeIdGenerator::new(1_725_215_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-compose-pointer"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let board = SessionBoard::new(session, Vec::new()).expect("board");
    let mut app = BoardApp::new(
        AppState::new(board),
        crate::adapters::editor::RopeEditorFactory,
    );
    let added_id = ids.thought_id();
    app.handle_control(
        &ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: added_id,
            content: "external".to_owned(),
            annotations: Vec::new(),
            position: None,
        },
        &FakeClock::new(Timestamp::from_millis(2)),
    )
    .expect("control add");
    assert!(app.compose_prompt_visible());
    let sequence = app.state.board.session.last_durable_sequence;
    let history = app.state.board_history().to_vec();
    let mut expected_ids = ids.clone();

    let effects = app.pointer_insert(
        PointerInput {
            column: 0,
            row: 0,
            kind: PointerKind::Down(PointerButton::Left),
            extend_selection: false,
        },
        &mut ids,
        &FakeClock::new(Timestamp::from_millis(3)),
    );

    assert!(effects.is_empty());
    assert_eq!(app.state.mode, InteractionMode::Compose);
    assert!(app.compose_editor_visible());
    assert_eq!(app.state.board.live_thoughts().len(), 1);
    assert_eq!(app.state.board.live_thoughts()[0].id, added_id);
    assert_eq!(app.state.board_history(), history.as_slice());
    assert_eq!(app.state.board.session.last_durable_sequence, sequence);
    assert_eq!(ids.thought_id(), expected_ids.thought_id());
    assert_eq!(ids.operation_id(), expected_ids.operation_id());
}
