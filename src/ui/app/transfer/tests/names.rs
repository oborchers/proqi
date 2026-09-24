//! Name-specific transfer ownership and stale-receipt contracts.

use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{Action, AppState, Effect},
    domain::{Session, SessionBoard, Thought, ThoughtName, ThoughtPosition, Timestamp},
    ports::environment::IdGenerator,
    ui::{BoardApp, UiKey, input::RoutedInput as UiInput},
};

#[test]
fn name_only_source_change_keeps_source_after_move_receipt() {
    let mut ids = FakeIdGenerator::new(1_725_203_500_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-stale-name"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let destination = ids.session_id();
    let mut thought = Thought::new(
        ids.thought_id(),
        source.id,
        "unchanged body".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    thought.set_name(Some(ThoughtName::new("Sent name").expect("name")));
    let thought_id = thought.id;
    let board = SessionBoard::new(source, vec![thought]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(1, Ok(vec![super::session_hit(destination)]));
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThoughts(request)] = effects.as_slice() else {
        panic!("expected transfer request");
    };
    assert_eq!(
        request.items[0].name.as_ref().map(ThoughtName::as_str),
        Some("Sent name")
    );

    let renamed = app.reduce(Action::RenameThought {
        operation_id: ids.operation_id(),
        thought_id,
        name: Some(ThoughtName::new("Newer local name").expect("name")),
        at: Timestamp::from_millis(4),
    });
    assert!(matches!(
        renamed.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    let result = super::successful_transfer(destination, request.operation_id);

    let completion = app.complete_session_transfer_batch(request, Ok(result), &mut ids, &clock);

    assert!(matches!(
        completion.as_slice(),
        [Effect::FinishTransfer { removal: None, .. }]
    ));
    let current = app
        .state
        .board
        .thought(thought_id)
        .expect("source retained");
    assert_eq!(current.content, "unchanged body");
    assert_eq!(
        current.name.as_ref().map(ThoughtName::as_str),
        Some("Newer local name")
    );
    assert_eq!(
        app.status_text(),
        Some("thought sent; source changed and was kept")
    );
}
