use super::*;

#[test]
fn editor_history_is_separate_from_board_history() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("before");
    let revision_id = fixture.ids.revision_id();
    let edit_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id,
            revision_id,
            before_content: "before".to_owned(),
            after_content: "after".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 5),
            at: edit_at,
        },
    )
    .expect("edit");
    assert_eq!(fixture.state.editor_history_cursor(thought_id), 1);

    move_history(&mut fixture, UndoScope::Editor { thought_id }, true);
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "before"
    );
    assert_eq!(fixture.state.board_history_cursor(), 1);

    move_history(&mut fixture, UndoScope::Editor { thought_id }, false);
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "after"
    );

    move_history(&mut fixture, UndoScope::Editor { thought_id }, true);
    move_history(&mut fixture, UndoScope::Board, true);
    assert!(
        !fixture
            .state
            .board
            .thought(thought_id)
            .expect("retained")
            .is_live()
    );
    assert_eq!(fixture.state.editor_history_cursor(thought_id), 0);

    let before = fixture.state.clone();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    assert!(
        reduce(
            &mut fixture.state,
            Action::Redo {
                operation_id,
                scope: UndoScope::Editor { thought_id },
                at,
            },
        )
        .is_err()
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn smart_continuation_and_exit_remain_two_persistent_editor_revisions() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("- first");
    for (before, after, before_cursor, after_cursor) in [
        (
            "- first",
            "- first\n- ",
            TextPosition::new(0, 7),
            TextPosition::new(1, 2),
        ),
        (
            "- first\n- ",
            "- first\n",
            TextPosition::new(1, 2),
            TextPosition::new(1, 0),
        ),
    ] {
        let revision_id = fixture.ids.revision_id();
        let at = fixture.time();
        reduce(
            &mut fixture.state,
            Action::EditThought {
                thought_id,
                revision_id,
                before_content: before.to_owned(),
                after_content: after.to_owned(),
                before_annotations: Vec::new(),
                after_annotations: Vec::new(),
                before_cursor,
                after_cursor,
                at,
            },
        )
        .expect("smart list revision");
    }
    assert_eq!(fixture.state.editor_history_cursor(thought_id), 2);
    move_history(&mut fixture, UndoScope::Editor { thought_id }, true);
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "- first\n- "
    );
    move_history(&mut fixture, UndoScope::Editor { thought_id }, true);
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "- first"
    );
}

#[test]
fn undoing_a_nonfocused_create_keeps_the_next_insertion_valid() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    fixture.create("second");
    reduce(&mut fixture.state, Action::FocusThought(Some(first))).expect("focus first");
    reduce(&mut fixture.state, Action::ExitEdit).expect("board mode");
    move_history(&mut fixture, UndoScope::Board, true);

    let third = fixture.create("third");
    assert_eq!(fixture.state.board.live_thoughts().len(), 2);
    assert_eq!(fixture.state.focused_thought, Some(third));
}

#[test]
fn storage_failure_remains_visible_until_retry() {
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
    let thought_id = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let blocked = reduce(
        &mut fixture.state,
        Action::CreateThought {
            thought_id,
            operation_id,
            content: "second".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at,
        },
    );
    assert!(blocked.is_err());
    assert_eq!(fixture.state.board.live_thoughts().len(), 1);
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Failed {
            durable: OperationSequence::ZERO,
            failed: OperationSequence::new(1),
            code: FailureCode::StorageFailed,
        }
    );
    let retry = reduce(
        &mut fixture.state,
        Action::RetryPersistence(OperationSequence::new(1)),
    )
    .expect("retry");
    assert_eq!(
        retry,
        vec![Effect::RetryPersistence {
            sequence: OperationSequence::new(1)
        }]
    );
    assert!(matches!(
        fixture.state.durability,
        DurabilityState::Pending { .. }
    ));
}

#[test]
fn concurrent_failures_keep_the_earliest_retry_boundary() {
    let mut fixture = Fixture::new();
    fixture.create("first");
    fixture.create("second");
    for sequence in [OperationSequence::new(2), OperationSequence::new(1)] {
        reduce(
            &mut fixture.state,
            Action::PersistenceFailed {
                sequence,
                code: FailureCode::StorageFailed,
            },
        )
        .expect("record failure");
    }
    assert!(matches!(
        fixture.state.durability,
        DurabilityState::Failed {
            failed,
            ..
        } if failed == OperationSequence::new(1)
    ));
}

#[test]
fn acknowledgements_must_be_ordered_and_truthful() {
    let mut fixture = Fixture::new();
    fixture.create("one");
    fixture.create("two");
    assert!(
        reduce(
            &mut fixture.state,
            Action::PersistenceCommitted(OperationSequence::new(2))
        )
        .is_err()
    );
    assert_eq!(
        fixture.state.board.session.last_durable_sequence,
        OperationSequence::ZERO
    );

    reduce(
        &mut fixture.state,
        Action::PersistenceCommitted(OperationSequence::new(1)),
    )
    .expect("first ack");
    assert_eq!(
        fixture.state.board.session.last_durable_sequence,
        OperationSequence::new(1)
    );
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Pending {
            durable: OperationSequence::new(1),
            latest: OperationSequence::new(2),
        }
    );
    reduce(
        &mut fixture.state,
        Action::PersistenceCommitted(OperationSequence::new(2)),
    )
    .expect("second ack");
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Durable {
            sequence: OperationSequence::new(2)
        }
    );
}

#[test]
fn invalid_reducer_action_leaves_state_unchanged() {
    let mut fixture = Fixture::new();
    let missing = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let before = fixture.state.clone();
    assert!(
        reduce(
            &mut fixture.state,
            Action::DeleteThought {
                operation_id,
                thought_id: missing,
                kind: BoardOperationKind::Delete,
                at,
            },
        )
        .is_err()
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn aggregate_rejects_invalid_mutation_transactionally() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("unchanged");
    let before = fixture.state.board.clone();
    let result = fixture.state.board.apply_mutation(
        &BoardMutation::MoveThought {
            thought_id,
            from: ThoughtPosition::new(0),
            to: ThoughtPosition::new(99),
        },
        Timestamp::from_millis(99),
    );
    assert!(result.is_err());
    assert_eq!(fixture.state.board, before);
}
