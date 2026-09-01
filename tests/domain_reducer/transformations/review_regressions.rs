use super::*;

#[test]
fn split_absorbs_a_dirty_editor_snapshot_into_one_board_history_unit() {
    let mut fixture = Fixture::new();
    let source = fixture.create("base");
    let neighbor = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let effects = reduce(
        &mut fixture.state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: neighbor,
            operation_id,
            expected_content: "base".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "base dirty".to_owned(),
            source_annotations: Vec::new(),
            at_byte: 4,
            at,
        },
    )
    .expect("atomic dirty split");
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        fixture
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        ["base", " dirty"]
    );

    move_history(&mut fixture, UndoScope::Board, true);
    assert_eq!(fixture.state.board.live_thoughts().len(), 1);
    assert_eq!(fixture.state.board.live_thoughts()[0].content, "base");
}

#[test]
fn merge_reports_a_concurrently_deleted_source_as_missing_without_mutation() {
    let mut fixture = Fixture::new();
    let first = fixture.create("one");
    let second = fixture.create("two");
    let expected_sources = [first, second]
        .into_iter()
        .map(|id| fixture.state.board.thought(id).expect("source").clone())
        .collect::<Vec<_>>();
    let delete_operation = fixture.operation_id();
    let deleted_at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::DeleteThought {
            operation_id: delete_operation,
            thought_id: second,
            kind: BoardOperationKind::Delete,
            at: deleted_at,
        },
    )
    .expect("concurrent deletion");
    let before = fixture.state.clone();
    let merge_operation = fixture.operation_id();
    let at = fixture.time();
    assert_eq!(
        reduce(
            &mut fixture.state,
            Action::MergeThoughts {
                operation_id: merge_operation,
                thought_ids: vec![first, second],
                expected_sources,
                separator: "\n\n".to_owned(),
                at,
            },
        ),
        Err(proqi::application::ApplicationError::ThoughtNotFound(
            second
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn undoing_a_split_returns_focus_to_its_retained_source_instead_of_board_start() {
    let mut fixture = Fixture::new();
    fixture.create("first");
    fixture.create("middle");
    let source = fixture.create("left right");
    let new_thought_id = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id,
            operation_id,
            expected_content: "left right".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "left right".to_owned(),
            source_annotations: Vec::new(),
            at_byte: 5,
            at,
        },
    )
    .expect("split");
    assert_eq!(fixture.state.focused_thought, Some(new_thought_id));

    move_history(&mut fixture, UndoScope::Board, true);

    assert_eq!(fixture.state.focused_thought, Some(source));
    assert_eq!(fixture.state.mode, InteractionMode::Board);
}

#[test]
fn incompatible_editor_undo_after_a_newer_board_change_is_actionable_and_exact() {
    let mut fixture = Fixture::new();
    let source = fixture.create("before");
    let revision_id = fixture.ids.revision_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id: source,
            revision_id,
            before_content: "before".to_owned(),
            after_content: "changed".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 6),
            after_cursor: TextPosition::new(0, 7),
            at,
        },
    )
    .expect("edit");
    let new_thought_id = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id,
            operation_id,
            expected_content: "changed".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "changed".to_owned(),
            source_annotations: Vec::new(),
            at_byte: 3,
            at,
        },
    )
    .expect("split");
    fixture.create("newer board change");
    reduce(&mut fixture.state, Action::EnterEdit(source)).expect("enter source editor");
    assert_eq!(
        fixture.state.preferred_undo_scope(fixture.state.mode),
        UndoScope::Editor { thought_id: source }
    );
    let before = fixture.state.clone();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let error = reduce(
        &mut fixture.state,
        Action::Undo {
            operation_id,
            scope: UndoScope::Editor { thought_id: source },
            at,
        },
    )
    .expect_err("incompatible editor undo");
    assert!(
        error
            .to_string()
            .contains("exit edit to undo newer board operations first")
    );
    assert_eq!(fixture.state, before);
}
