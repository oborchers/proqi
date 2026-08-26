use super::*;

#[test]
fn paste_creates_one_focused_thought_and_one_commit_effect() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let effects = reduce(
        &mut fixture.state,
        Action::PasteAsThought {
            thought_id,
            operation_id,
            content: "Grüße\r\n日本語".to_owned(),
            annotations: Vec::new(),
            at,
        },
    )
    .expect("paste");

    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], Effect::CommitBoardOperation(_)));
    assert_eq!(fixture.state.focused_thought, Some(thought_id));
    assert_eq!(fixture.state.mode, InteractionMode::Edit { thought_id });
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "Grüße\r\n日本語"
    );
    assert_eq!(
        fixture.state.durability,
        DurabilityState::Pending {
            durable: OperationSequence::ZERO,
            latest: OperationSequence::new(1),
        }
    );
}

#[test]
fn copy_is_exact_and_never_mutates_the_board() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("  exact\r\n");
    let request_id = fixture.ids.request_id();
    let effects = reduce(
        &mut fixture.state,
        Action::CopyThoughts {
            request_id,
            thought_ids: vec![thought_id],
        },
    )
    .expect("copy");
    assert_eq!(
        effects,
        vec![Effect::WriteClipboard {
            request_id,
            thought_id,
            intent: ClipboardIntent::Copy,
            content: "  exact\r\n".to_owned(),
        }]
    );
    reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("clipboard result");
    assert!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );
}

#[test]
fn cut_deletes_only_after_clipboard_success() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("keep until copied");

    let failed_request = fixture.ids.request_id();
    let failed_operation = fixture.operation_id();
    let failed_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::CutThoughts {
            request_id: failed_request,
            operation_id: failed_operation,
            thought_ids: vec![thought_id],
            at: failed_at,
        },
    )
    .expect("request cut");
    let failure = reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id: failed_request,
            result: Err(FailureCode::ClipboardFailed),
        },
    )
    .expect("failure");
    assert_eq!(
        failure,
        vec![Effect::Notify {
            code: FailureCode::ClipboardFailed
        }]
    );
    assert!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );

    let request_id = fixture.ids.request_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::CutThoughts {
            request_id,
            operation_id,
            thought_ids: vec![thought_id],
            at,
        },
    )
    .expect("request cut");
    let success = reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("success");
    assert!(
        matches!(success.as_slice(), [Effect::CommitBoardOperation(operation)] if operation.kind == BoardOperationKind::Cut)
    );
    assert!(
        !fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );
}

#[test]
fn board_reorder_delete_and_collapse_have_independent_undo_redo() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let second = fixture.create("second");
    let move_operation = fixture.operation_id();
    let move_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::MoveThought {
            operation_id: move_operation,
            thought_id: second,
            to: 0,
            at: move_at,
        },
    )
    .expect("move");
    assert_eq!(fixture.state.board.live_thoughts()[0].id, second);

    let collapse_operation = fixture.operation_id();
    let collapse_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::SetPresentation {
            operation_id: collapse_operation,
            thought_id: first,
            presentation: proqi::domain::ThoughtPresentation::Collapsed,
            at: collapse_at,
        },
    )
    .expect("collapse");
    assert_presentation(&fixture, first, ThoughtPresentation::Collapsed);

    move_history(&mut fixture, UndoScope::Board, true);
    assert_presentation(&fixture, first, ThoughtPresentation::Automatic);

    move_history(&mut fixture, UndoScope::Board, false);
    assert_presentation(&fixture, first, ThoughtPresentation::Collapsed);
    fixture.state.board.validate().expect("normalized board");

    let delete_operation = fixture.operation_id();
    let delete_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id: delete_operation,
            thought_id: second,
            kind: BoardOperationKind::Delete,
            at: delete_at,
        },
    )
    .expect("delete");
    assert!(
        !fixture
            .state
            .board
            .thought(second)
            .expect("retained")
            .is_live()
    );

    move_history(&mut fixture, UndoScope::Board, true);
    assert!(
        fixture
            .state
            .board
            .thought(second)
            .expect("restored")
            .is_live()
    );
    assert_eq!(fixture.state.board.live_thoughts()[0].id, second);
}

fn assert_presentation(fixture: &Fixture, thought_id: ThoughtId, expected: ThoughtPresentation) {
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .presentation,
        expected
    );
}

#[test]
fn deleting_the_focused_thought_preserves_its_board_position() {
    let mut fixture = Fixture::new();
    let first = fixture.create("first");
    let second = fixture.create("second");
    let third = fixture.create("third");
    reduce(&mut fixture.state, Action::FocusThought(Some(second))).expect("focus second");

    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id,
            thought_id: second,
            kind: BoardOperationKind::Delete,
            at,
        },
    )
    .expect("delete middle");
    assert_eq!(fixture.state.focused_thought, Some(third));

    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id,
            thought_id: third,
            kind: BoardOperationKind::Delete,
            at,
        },
    )
    .expect("delete last");
    assert_eq!(fixture.state.focused_thought, Some(first));

    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id,
            thought_id: first,
            kind: BoardOperationKind::Delete,
            at,
        },
    )
    .expect("delete only remaining thought");
    assert_eq!(fixture.state.focused_thought, None);
}
