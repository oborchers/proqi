use super::*;

#[test]
fn edits_to_restored_sources_invalidate_merge_redo_without_hiding_content() {
    let mut fixture = Fixture::new();
    let first = fixture.create("A");
    let second = fixture.create("B");
    let expected_sources = [first, second]
        .into_iter()
        .map(|id| fixture.state.board.thought(id).expect("source").clone())
        .collect();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::MergeThoughts {
            operation_id,
            thought_ids: vec![first, second],
            expected_sources,
            separator: "\n\n".to_owned(),
            at,
        },
    )
    .expect("merge");
    move_history(&mut fixture, UndoScope::Board, true);

    let revision_id = fixture.ids.revision_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::EditThought {
            thought_id: second,
            revision_id,
            before_content: "B".to_owned(),
            after_content: "B changed".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 1),
            after_cursor: TextPosition::new(0, 9),
            at,
        },
    )
    .expect("edit restored source");

    let before = fixture.state.clone();
    let redo_id = fixture.operation_id();
    let redo_at = fixture.time();
    assert!(
        reduce(
            &mut fixture.state,
            Action::Redo {
                operation_id: redo_id,
                scope: UndoScope::Board,
                at: redo_at,
            },
        )
        .is_err()
    );
    assert_eq!(fixture.state, before);
    assert_eq!(
        fixture.state.board.thought(second).expect("second").content,
        "B changed"
    );
}

#[test]
fn edits_to_restored_split_and_extract_sources_invalidate_their_redo() {
    for extract in [false, true] {
        let mut fixture = Fixture::new();
        let source = fixture.create("left right");
        let new_thought_id = fixture.ids.thought_id();
        let operation_id = fixture.operation_id();
        let at = fixture.time();
        let action = if extract {
            Action::ExtractThought {
                thought_id: source,
                new_thought_id,
                operation_id,
                expected_content: "left right".to_owned(),
                expected_annotations: Vec::new(),
                range: 5..6,
                at,
            }
        } else {
            Action::SplitThought {
                thought_id: source,
                new_thought_id,
                operation_id,
                expected_content: "left right".to_owned(),
                expected_annotations: Vec::new(),
                at_byte: 5,
                at,
            }
        };
        reduce(&mut fixture.state, action).expect("transform");
        move_history(&mut fixture, UndoScope::Board, true);

        let revision_id = fixture.ids.revision_id();
        let at = fixture.time();
        reduce(
            &mut fixture.state,
            Action::EditThought {
                thought_id: source,
                revision_id,
                before_content: "left right".to_owned(),
                after_content: "changed".to_owned(),
                before_annotations: Vec::new(),
                after_annotations: Vec::new(),
                before_cursor: TextPosition::default(),
                after_cursor: TextPosition::new(0, 7),
                at,
            },
        )
        .expect("edit restored source");
        let redo_id = fixture.operation_id();
        let redo_at = fixture.time();
        assert!(
            reduce(
                &mut fixture.state,
                Action::Redo {
                    operation_id: redo_id,
                    scope: UndoScope::Board,
                    at: redo_at,
                },
            )
            .is_err(),
            "{} redo must be invalidated",
            if extract { "extract" } else { "split" }
        );
        assert_eq!(
            fixture.state.board.thought(source).expect("source").content,
            "changed"
        );
    }
}
