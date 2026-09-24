use super::*;
use crate::ports::transfer::TransferItem;

#[test]
fn active_owner_preserves_complete_selected_cohort_in_one_board_operation() {
    let mut ids = FakeIdGenerator::new(1_725_209_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-active-selected-transfer"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    let original = Thought::new(
        ids.thought_id(),
        session.id,
        "original".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let original_id = original.id;
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, vec![original]).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    app.state.focused_item = Some(original_id.into());
    let items = (0..2)
        .map(|index| TransferItem {
            source_thought_id: ids.thought_id(),
            destination_thought_id: ids.thought_id(),
            content: format!("selected {index}"),
            annotations: Vec::new(),
            name: None,
        })
        .collect::<Vec<_>>();
    let effects = app
        .handle_control(
            &ControlMutation::PreserveAddMany {
                operation_id: ids.operation_id(),
                items: items.clone(),
            },
            &FakeClock::new(Timestamp::from_millis(2)),
        )
        .expect("active batch");
    assert!(
        matches!(effects.as_slice(), [Effect::CommitBoardOperation(operation)]
        if matches!(&operation.forward, BoardMutation::Batch { mutations } if mutations.len() == 2))
    );
    assert_eq!(app.state.focused_item, Some(original_id.into()));
    assert_eq!(
        app.state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>(),
        [
            original_id,
            items[0].destination_thought_id,
            items[1].destination_thought_id
        ]
    );
}
