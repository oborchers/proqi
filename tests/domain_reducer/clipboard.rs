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
            thought_id: Some(thought_id),
            intent: ClipboardIntent::Copy,
            content: "  exact\r\n".to_owned(),
            annotations: Vec::new(),
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
fn cut_success_does_not_delete_an_intervening_annotated_edit() {
    let mut fixture = Fixture::new();
    let thought_id = fixture.create("/tmp/original.png");
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
    .expect("cut request");

    let before = "/tmp/original.png".to_owned();
    let after = "/tmp/replacement.png".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: after.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "replacement.png".to_owned(),
        },
    };
    let revision_id = fixture.ids.revision_id();
    let edit_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id,
            revision_id,
            before_content: before,
            after_content: after.clone(),
            before_annotations: Vec::new(),
            after_annotations: vec![annotation.clone()],
            before_cursor: TextPosition::default(),
            after_cursor: TextPosition::default(),
            at: edit_at,
        },
    )
    .expect("intervening edit");

    let completion = reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("clipboard result");
    assert_eq!(
        completion,
        vec![Effect::Notify {
            code: FailureCode::ContentConflict
        }]
    );
    let thought = fixture.state.board.thought(thought_id).expect("thought");
    assert!(thought.is_live());
    assert_eq!(thought.content, after);
    assert_eq!(thought.annotations, vec![annotation]);
}

#[test]
fn annotated_cut_restores_exact_metadata_in_one_board_undo() {
    let mut fixture = Fixture::new();
    let path = "/missing/Grüße.png";
    let thought_id = fixture.create(path);
    let annotation = ContentAnnotation {
        start: 0,
        end: path.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "Grüße.png".to_owned(),
        },
    };
    fixture
        .state
        .board
        .thought_mut(thought_id)
        .expect("thought")
        .set_annotations(vec![annotation.clone()])
        .expect("annotations");
    let request_id = fixture.ids.request_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let effects = reduce(
        &mut fixture.state,
        Action::CutThoughts {
            request_id,
            operation_id,
            thought_ids: vec![thought_id],
            at,
        },
    )
    .expect("cut request");
    let write = effects.iter().find(|effect| {
        matches!(
            effect,
            Effect::WriteClipboard { content, annotations, .. }
                if content == path && annotations == std::slice::from_ref(&annotation)
        )
    });
    assert!(write.is_some());
    reduce(
        &mut fixture.state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("cut success");
    assert!(
        !fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .is_live()
    );

    move_history(&mut fixture, UndoScope::Board, true);
    let restored = fixture.state.board.thought(thought_id).expect("restored");
    assert!(restored.is_live());
    assert_eq!(restored.content, path);
    assert_eq!(restored.annotations, vec![annotation]);
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
