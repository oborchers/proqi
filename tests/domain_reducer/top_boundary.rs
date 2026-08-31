//! Durable insertion ordering and history for a create before the first thought.

use super::*;

#[test]
fn insertion_at_zero_preserves_existing_identity_and_round_trips_history() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let second = fixture.create("second");
    let blank = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();

    let effects = reduce(
        &mut fixture.state,
        Action::CreateThought {
            thought_id: blank,
            operation_id,
            content: String::new(),
            annotations: Vec::new(),
            insertion_index: Some(0),
            at,
        },
    )
    .expect("insert at top");
    assert_eq!(effects.len(), 1);
    assert_eq!(
        fixture
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>(),
        [blank, first, second]
    );
    assert_eq!(fixture.state.board_history_cursor(), 3);

    move_history(&mut fixture, UndoScope::Board, true);
    assert_eq!(
        fixture
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>(),
        [first, second]
    );
    move_history(&mut fixture, UndoScope::Board, false);
    assert_eq!(fixture.state.board.live_thoughts()[0].id, blank);
}

#[test]
fn failed_storage_blocks_a_second_top_create_without_duplicate_sequences() {
    let mut fixture = Fixture::new();
    fixture.create("first");
    reduce(
        &mut fixture.state,
        Action::PersistenceFailed {
            sequence: OperationSequence::new(1),
            code: FailureCode::StorageFailed,
        },
    )
    .expect("record failure");
    let action = Action::CreateThought {
        thought_id: fixture.ids.thought_id(),
        operation_id: fixture.operation_id(),
        content: String::new(),
        annotations: Vec::new(),
        insertion_index: Some(0),
        at: fixture.time(),
    };

    assert!(reduce(&mut fixture.state, action).is_err());
    assert_eq!(fixture.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.state.board_history_cursor(), 1);
}
