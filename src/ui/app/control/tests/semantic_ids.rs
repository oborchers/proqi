use crate::{
    adapters::memory::{FakeClock, FakeIdGenerator},
    application::{AppState, ApplicationError},
    domain::{Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{control::ControlMutation, environment::IdGenerator},
};

use super::BoardApp;

fn app_with_thought(ids: &mut FakeIdGenerator) -> (BoardApp, crate::domain::ThoughtId) {
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-semantic-identities"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session.id,
        "source".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    (
        BoardApp::new(
            AppState::new(board),
            crate::adapters::editor::RopeEditorFactory,
        ),
        thought_id,
    )
}

#[test]
fn active_owner_rejects_noncanonical_semantic_result_ids_without_state_change() {
    let mut ids = FakeIdGenerator::new(1_725_230_000_000);
    let (mut app, thought_id) = app_with_thought(&mut ids);
    let original = app.state.clone();
    let clock = FakeClock::new(Timestamp::from_millis(2));

    let insert = ControlMutation::InsertSeparator {
        operation_id: ids.operation_id(),
        separator_id: ids.separator_id(),
        position: None,
    };
    assert_eq!(
        app.handle_control(&insert, &clock),
        Err(ApplicationError::InvalidState)
    );
    assert_eq!(app.state, original);

    let split = ControlMutation::SplitThought {
        operation_id: ids.operation_id(),
        thought_id,
        new_thought_id: ids.thought_id(),
        expected_digest: [0; 32],
        at_byte: 3,
    };
    assert_eq!(
        app.handle_control(&split, &clock),
        Err(ApplicationError::InvalidState)
    );
    assert_eq!(app.state, original);

    let duplicate = ControlMutation::DuplicateItems {
        operation_id: ids.operation_id(),
        item_ids: vec![thought_id.into()],
        duplicate_ids: vec![ids.thought_id().into()],
    };
    assert_eq!(
        app.handle_control(&duplicate, &clock),
        Err(ApplicationError::InvalidState)
    );
    assert_eq!(app.state, original);
}

#[test]
fn active_owner_canonicalizes_merge_to_its_launch_time_separator() {
    let mut ids = FakeIdGenerator::new(1_725_231_000_000);
    let (app, thought_id) = app_with_thought(&mut ids);
    let mutation = ControlMutation::MergeThoughts {
        operation_id: ids.operation_id(),
        thought_ids: vec![thought_id, ids.thought_id()],
        expected_digests: vec![[0; 32], [1; 32]],
        separator: "caller config changed".to_owned(),
    };

    let ControlMutation::MergeThoughts { separator, .. } =
        app.canonical_control_mutation(&mutation)
    else {
        panic!("merge mutation");
    };
    assert_eq!(separator, "\n\n");
}
