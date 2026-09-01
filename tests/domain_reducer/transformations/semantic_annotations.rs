use super::*;

fn shortcut(start: usize, end: usize) -> ContentAnnotation {
    serde_json::from_value(serde_json::json!({
        "start": start,
        "end": end,
        "kind": { "kind": "shortcut_emphasis" }
    }))
    .expect("valid application-owned shortcut annotation")
}

#[test]
fn split_preserves_a_complete_shortcut_through_undo_and_redo() {
    let mut split = Fixture::new();
    let source = split.create("AA Enter ZZ");
    split
        .state
        .board
        .thought_mut(source)
        .expect("source")
        .annotations = vec![shortcut(3, 8)];
    let right = split.ids.thought_id();
    let operation_id = split.operation_id();
    let at = split.time();
    reduce(
        &mut split.state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: right,
            operation_id,
            expected_content: "AA Enter ZZ".to_owned(),
            expected_annotations: vec![shortcut(3, 8)],
            at_byte: 3,
            at,
        },
    )
    .expect("split before shortcut");
    assert_eq!(
        split.state.board.thought(right).expect("right").annotations,
        [shortcut(0, 5)]
    );
    move_history(&mut split, UndoScope::Board, true);
    assert_eq!(
        split
            .state
            .board
            .thought(source)
            .expect("restored")
            .annotations,
        [shortcut(3, 8)]
    );
    move_history(&mut split, UndoScope::Board, false);
    assert_eq!(
        split
            .state
            .board
            .thought(right)
            .expect("redone")
            .annotations,
        [shortcut(0, 5)]
    );
}

#[test]
fn split_dissolves_a_crossing_shortcut_range() {
    let mut crossing = Fixture::new();
    let source = crossing.create("AA Enter ZZ");
    crossing
        .state
        .board
        .thought_mut(source)
        .expect("source")
        .annotations = vec![shortcut(3, 8)];
    let crossing_right = crossing.ids.thought_id();
    let operation_id = crossing.operation_id();
    let at = crossing.time();
    reduce(
        &mut crossing.state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: crossing_right,
            operation_id,
            expected_content: "AA Enter ZZ".to_owned(),
            expected_annotations: vec![shortcut(3, 8)],
            at_byte: 5,
            at,
        },
    )
    .expect("split through shortcut");
    assert!(
        crossing
            .state
            .board
            .live_thoughts()
            .iter()
            .all(|thought| thought.annotations.is_empty())
    );
}

#[test]
fn merge_shifts_and_preserves_a_complete_shortcut_range() {
    let mut merge = Fixture::new();
    let first = merge.create("AA");
    let second = merge.create("Enter");
    merge
        .state
        .board
        .thought_mut(second)
        .expect("second")
        .annotations = vec![shortcut(0, 5)];
    let expected_sources = [first, second]
        .into_iter()
        .map(|id| merge.state.board.thought(id).expect("source").clone())
        .collect();
    let operation_id = merge.operation_id();
    let at = merge.time();
    reduce(
        &mut merge.state,
        Action::MergeThoughts {
            operation_id,
            thought_ids: vec![first, second],
            expected_sources,
            separator: "\n\n".to_owned(),
            at,
        },
    )
    .expect("merge shortcut");
    assert_eq!(
        merge
            .state
            .board
            .thought(first)
            .expect("survivor")
            .annotations,
        [shortcut(4, 9)]
    );
}

#[test]
fn extraction_moves_a_complete_shortcut_and_dissolves_a_crossing_one() {
    for range in [3..8, 5..9] {
        let mut fixture = Fixture::new();
        let source = fixture.create("AA Enter ZZ");
        fixture
            .state
            .board
            .thought_mut(source)
            .expect("source")
            .annotations = vec![shortcut(3, 8)];
        let extracted = fixture.ids.thought_id();
        let operation_id = fixture.operation_id();
        let at = fixture.time();
        reduce(
            &mut fixture.state,
            Action::ExtractThought {
                thought_id: source,
                new_thought_id: extracted,
                operation_id,
                expected_content: "AA Enter ZZ".to_owned(),
                expected_annotations: vec![shortcut(3, 8)],
                range: range.clone(),
                at,
            },
        )
        .expect("extract shortcut range");
        let annotations = &fixture
            .state
            .board
            .thought(extracted)
            .expect("extracted")
            .annotations;
        if range == (3..8) {
            assert_eq!(annotations, &[shortcut(0, 5)]);
        } else {
            assert!(annotations.is_empty());
            assert!(
                fixture
                    .state
                    .board
                    .thought(source)
                    .expect("remaining")
                    .annotations
                    .is_empty()
            );
        }
    }
}
