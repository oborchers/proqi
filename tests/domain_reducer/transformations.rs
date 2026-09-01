use super::*;
use proqi::domain::{ContentAnnotation, ContentAnnotationKind};

#[path = "transformations/review_regressions.rs"]
mod review_regressions;
#[path = "transformations/semantic_annotations.rs"]
mod semantic_annotations;
#[path = "transformations/stale_redo.rs"]
mod stale_redo;

fn folded(start: usize, end: usize) -> ContentAnnotation {
    ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::LargePaste {
            lines: 2,
            graphemes: end - start,
        },
    }
}

fn create_annotated(
    fixture: &mut Fixture,
    content: &str,
    annotations: Vec<ContentAnnotation>,
) -> ThoughtId {
    let thought_id = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::CreateThought {
            thought_id,
            operation_id,
            content: content.to_owned(),
            annotations,
            insertion_index: None,
            at,
        },
    )
    .expect("create annotated");
    thought_id
}

#[test]
fn split_keeps_left_identity_and_exact_untrimmed_right_at_every_boundary() {
    for at_byte in [0, 4, "left\r\n右".len()] {
        let mut fixture = Fixture::new();
        let source = create_annotated(
            &mut fixture,
            "left\r\n右",
            vec![folded(2, "left\r\n右".len())],
        );
        let new = fixture.ids.thought_id();
        let operation_id = fixture.operation_id();
        let at = fixture.time();
        let effects = reduce(
            &mut fixture.state,
            Action::SplitThought {
                thought_id: source,
                new_thought_id: new,
                operation_id,
                expected_content: "left\r\n右".to_owned(),
                expected_annotations: vec![folded(2, "left\r\n右".len())],
                source_content: "left\r\n右".to_owned(),
                source_annotations: vec![folded(2, "left\r\n右".len())],
                at_byte,
                at,
            },
        )
        .expect("split");
        assert!(matches!(
            effects.as_slice(),
            [Effect::CommitBoardOperation(_)]
        ));
        let live = fixture.state.board.live_thoughts();
        assert_eq!(live.len(), 2);
        assert_eq!(live[0].id, source);
        assert_eq!(live[0].content, &"left\r\n右"[..at_byte]);
        assert_eq!(live[1].id, new);
        assert_eq!(live[1].content, &"left\r\n右"[at_byte..]);
        assert_eq!(fixture.state.focused_thought, Some(new));
        assert_eq!(
            fixture.state.mode,
            InteractionMode::Edit { thought_id: new }
        );

        move_history(&mut fixture, UndoScope::Board, true);
        assert_eq!(
            fixture.state.board.thought(source).expect("source").content,
            "left\r\n右"
        );
        assert!(
            !fixture
                .state
                .board
                .thought(new)
                .expect("retained")
                .is_live()
        );
        move_history(&mut fixture, UndoScope::Board, false);
        assert_eq!(fixture.state.board.live_thoughts().len(), 2);
    }
}

#[test]
fn extract_closes_only_the_exact_range_and_dissolves_crossing_annotation() {
    let mut fixture = Fixture::new();
    let source = create_annotated(&mut fixture, "ab日本cd", vec![folded(1, 10)]);
    let new = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::ExtractThought {
            thought_id: source,
            new_thought_id: new,
            operation_id,
            expected_content: "ab日本cd".to_owned(),
            expected_annotations: vec![folded(1, 10)],
            source_content: "ab日本cd".to_owned(),
            source_annotations: vec![folded(1, 10)],
            range: 2..8,
            at,
        },
    )
    .expect("extract");
    let source = fixture.state.board.thought(source).expect("source");
    let extracted = fixture.state.board.thought(new).expect("extracted");
    assert_eq!(source.content, "abcd");
    assert!(source.annotations.is_empty());
    assert_eq!(extracted.content, "日本");
    assert!(extracted.annotations.is_empty());
}

#[test]
fn transformations_reject_stale_empty_and_noncontiguous_inputs_without_mutation() {
    let mut fixture = Fixture::new();
    let first = fixture.create("one");
    let middle = fixture.create("two");
    let last = fixture.create("three");

    let before = fixture.state.clone();
    let stale_operation = fixture.operation_id();
    let stale_new = fixture.ids.thought_id();
    let stale_at = fixture.time();
    assert_eq!(
        reduce(
            &mut fixture.state,
            Action::SplitThought {
                thought_id: first,
                new_thought_id: stale_new,
                operation_id: stale_operation,
                expected_content: "stale".to_owned(),
                expected_annotations: Vec::new(),
                source_content: "stale".to_owned(),
                source_annotations: Vec::new(),
                at_byte: 0,
                at: stale_at,
            },
        ),
        Err(proqi::application::ApplicationError::ContentConflict(first))
    );
    assert_eq!(fixture.state, before);

    let empty_operation = fixture.operation_id();
    let empty_new = fixture.ids.thought_id();
    let empty_at = fixture.time();
    assert!(
        reduce(
            &mut fixture.state,
            Action::ExtractThought {
                thought_id: middle,
                new_thought_id: empty_new,
                operation_id: empty_operation,
                expected_content: "two".to_owned(),
                expected_annotations: Vec::new(),
                source_content: "two".to_owned(),
                source_annotations: Vec::new(),
                range: 1..1,
                at: empty_at,
            },
        )
        .is_err()
    );
    assert_eq!(fixture.state, before);

    let merge_operation = fixture.operation_id();
    let merge_at = fixture.time();
    let noncontiguous_sources = [first, last]
        .into_iter()
        .map(|id| fixture.state.board.thought(id).expect("source").clone())
        .collect();
    assert_eq!(
        reduce(
            &mut fixture.state,
            Action::MergeThoughts {
                operation_id: merge_operation,
                thought_ids: vec![first, last],
                expected_sources: noncontiguous_sources,
                separator: "\n\n".to_owned(),
                at: merge_at,
            },
        ),
        Err(proqi::application::ApplicationError::NoncontiguousSelection)
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn merge_rejects_a_stale_source_snapshot_without_mutation() {
    let mut fixture = Fixture::new();
    let first = fixture.create("one");
    let second = fixture.create("two");
    let before = fixture.state.clone();
    let mut stale_sources = [first, second]
        .into_iter()
        .map(|id| fixture.state.board.thought(id).expect("source").clone())
        .collect::<Vec<_>>();
    stale_sources[1].content.push('!');
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    assert_eq!(
        reduce(
            &mut fixture.state,
            Action::MergeThoughts {
                operation_id,
                thought_ids: vec![first, second],
                expected_sources: stale_sources,
                separator: "\n\n".to_owned(),
                at,
            },
        ),
        Err(proqi::application::ApplicationError::ContentConflict(
            second
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn every_transformation_rejects_locked_sources_without_mutation() {
    let mut fixture = Fixture::new();
    let first = fixture.create("one");
    let second = fixture.create("two");
    reduce(
        &mut fixture.state,
        Action::BeginSubmission {
            thought_ids: vec![first],
        },
    )
    .expect("lock source");
    let before = fixture.state.clone();
    let expected_sources = [first, second]
        .into_iter()
        .map(|id| fixture.state.board.thought(id).expect("source").clone())
        .collect::<Vec<_>>();

    let split_operation = fixture.operation_id();
    let split_new = fixture.ids.thought_id();
    let split_at = fixture.time();
    let extract_operation = fixture.operation_id();
    let extract_new = fixture.ids.thought_id();
    let extract_at = fixture.time();
    let merge_operation = fixture.operation_id();
    let merge_at = fixture.time();
    let cases = [
        Action::SplitThought {
            thought_id: first,
            new_thought_id: split_new,
            operation_id: split_operation,
            expected_content: "one".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "one".to_owned(),
            source_annotations: Vec::new(),
            at_byte: 1,
            at: split_at,
        },
        Action::ExtractThought {
            thought_id: first,
            new_thought_id: extract_new,
            operation_id: extract_operation,
            expected_content: "one".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "one".to_owned(),
            source_annotations: Vec::new(),
            range: 0..1,
            at: extract_at,
        },
        Action::MergeThoughts {
            operation_id: merge_operation,
            thought_ids: vec![first, second],
            expected_sources,
            separator: "\n\n".to_owned(),
            at: merge_at,
        },
    ];
    for action in cases {
        assert_eq!(
            reduce(&mut fixture.state, action),
            Err(proqi::application::ApplicationError::ThoughtLocked(first))
        );
        assert_eq!(fixture.state, before);
    }
}

#[test]
fn merge_keeps_first_identity_exact_separator_and_recoverable_sources() {
    let mut fixture = Fixture::new();
    let first = create_annotated(&mut fixture, "one", vec![folded(0, 3)]);
    let second = fixture.create("");
    let third = create_annotated(&mut fixture, "三", vec![folded(0, 3)]);
    let operation = fixture.operation_id();
    let at = fixture.time();
    let expected_sources = [first, second, third]
        .into_iter()
        .map(|id| fixture.state.board.thought(id).expect("source").clone())
        .collect();
    reduce(
        &mut fixture.state,
        Action::MergeThoughts {
            operation_id: operation,
            thought_ids: vec![first, second, third],
            expected_sources,
            separator: "\r\n\r\n".to_owned(),
            at,
        },
    )
    .expect("merge");
    assert_eq!(fixture.state.board.live_thoughts().len(), 1);
    let survivor = fixture.state.board.thought(first).expect("survivor");
    assert_eq!(survivor.content, "one\r\n\r\n\r\n\r\n三");
    assert_eq!(survivor.annotations, vec![folded(0, 3), folded(11, 14)]);
    assert!(
        !fixture
            .state
            .board
            .thought(second)
            .expect("second")
            .is_live()
    );
    assert!(!fixture.state.board.thought(third).expect("third").is_live());

    move_history(&mut fixture, UndoScope::Board, true);
    assert_eq!(
        fixture
            .state
            .board
            .live_thoughts()
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "", "三"]
    );
    move_history(&mut fixture, UndoScope::Board, false);
    assert_eq!(
        fixture.state.board.live_thoughts()[0].content,
        "one\r\n\r\n\r\n\r\n三"
    );
}

#[test]
fn repeated_large_unicode_split_and_merge_round_trips_exactly() {
    let mut fixture = Fixture::new();
    let content = "界\r\ncontrol\tdata\n".repeat(4_096);
    let source = fixture.create(&content);
    for _ in 0..12 {
        let neighbor = fixture.ids.thought_id();
        let split_operation = fixture.operation_id();
        let split_at = fixture.time();
        reduce(
            &mut fixture.state,
            Action::SplitThought {
                thought_id: source,
                new_thought_id: neighbor,
                operation_id: split_operation,
                expected_content: content.clone(),
                expected_annotations: Vec::new(),
                source_content: content.clone(),
                source_annotations: Vec::new(),
                at_byte: "界\r\n".len(),
                at: split_at,
            },
        )
        .expect("repeated split");
        let expected_sources = [source, neighbor]
            .into_iter()
            .map(|id| fixture.state.board.thought(id).expect("source").clone())
            .collect();
        let merge_operation = fixture.operation_id();
        let merge_at = fixture.time();
        reduce(
            &mut fixture.state,
            Action::MergeThoughts {
                operation_id: merge_operation,
                thought_ids: vec![source, neighbor],
                expected_sources,
                separator: String::new(),
                at: merge_at,
            },
        )
        .expect("repeated merge");
        assert_eq!(fixture.state.board.live_thoughts().len(), 1);
        assert_eq!(
            fixture
                .state
                .board
                .thought(source)
                .expect("survivor")
                .content,
            content
        );
    }
}
