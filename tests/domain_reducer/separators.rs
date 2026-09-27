use super::*;

fn live_ids(fixture: &Fixture) -> Vec<BoardItemId> {
    fixture
        .state
        .board
        .live_items()
        .into_iter()
        .map(proqi::domain::BoardItemRef::id)
        .collect()
}

#[test]
fn separators_keep_distinct_identity_through_move_delete_undo_and_redo() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let separator_a = fixture.insert_separator(1);
    let separator_b = fixture.insert_separator(2);
    let second = fixture.create("second");
    assert_eq!(
        live_ids(&fixture),
        vec![
            first.into(),
            separator_a.into(),
            separator_b.into(),
            second.into()
        ]
    );

    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::MoveItem {
            operation_id,
            item_id: separator_b.into(),
            to: 0,
            at,
        },
    )
    .expect("move separator");
    assert_eq!(
        live_ids(&fixture),
        vec![
            separator_b.into(),
            first.into(),
            separator_a.into(),
            second.into()
        ]
    );

    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteItems {
            operation_id,
            item_ids: vec![separator_b.into(), separator_a.into()],
            kind: BoardOperationKind::Delete,
            at,
        },
    )
    .expect("delete separators");
    assert_eq!(live_ids(&fixture), vec![first.into(), second.into()]);
    assert!(fixture.state.board.separator(separator_a).is_some());
    assert!(fixture.state.board.separator(separator_b).is_some());

    move_history(&mut fixture, UndoScope::Board, true);
    assert_eq!(
        live_ids(&fixture),
        vec![
            separator_b.into(),
            first.into(),
            separator_a.into(),
            second.into()
        ]
    );
    move_history(&mut fixture, UndoScope::Board, false);
    assert_eq!(live_ids(&fixture), vec![first.into(), second.into()]);
}

#[test]
fn separator_board_history_does_not_consume_editor_history() {
    let mut fixture = Fixture::new();
    let thought = fixture.create("before");
    let revision_id = fixture.ids.revision_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id: thought,
            revision_id,
            before_content: "before".to_owned(),
            after_content: "after".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 5),
            at,
        },
    )
    .expect("edit");
    let separator = fixture.insert_separator(1);

    move_history(&mut fixture, UndoScope::Board, true);
    assert!(
        !fixture
            .state
            .board
            .separator(separator)
            .expect("retained")
            .is_live()
    );
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought)
            .expect("thought")
            .content,
        "after"
    );
    move_history(
        &mut fixture,
        UndoScope::Editor {
            thought_id: thought,
        },
        true,
    );
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought)
            .expect("thought")
            .content,
        "before"
    );
    move_history(&mut fixture, UndoScope::Board, false);
    assert!(
        fixture
            .state
            .board
            .separator(separator)
            .expect("retained")
            .is_live()
    );
}

#[test]
fn separator_has_no_text_payload_in_json() {
    let mut fixture = Fixture::new();
    let separator = fixture.insert_separator(0);
    let value = serde_json::to_value(fixture.state.board.separator(separator).expect("separator"))
        .expect("serialize");
    let object = value.as_object().expect("separator object");
    assert_eq!(object.get("id"), Some(&serde_json::json!(separator)));
    assert!(!object.contains_key("content"));
    assert!(!object.contains_key("annotations"));
    assert!(!object.contains_key("presentation"));
}
