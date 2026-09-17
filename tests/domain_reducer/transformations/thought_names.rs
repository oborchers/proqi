//! Metadata semantics for content-derived and merged thoughts.

use super::*;
use proqi::domain::ThoughtName;

#[test]
fn derived_thoughts_start_unnamed_and_merge_keeps_the_survivor_name() {
    let mut fixture = Fixture::new();
    let source = fixture.create("left right");
    fixture
        .state
        .board
        .thought_mut(source)
        .expect("source")
        .set_name(Some(ThoughtName::new("Source label").expect("name")));
    let split = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: split,
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
    assert_name(&fixture, source, Some("Source label"));
    assert_name(&fixture, split, None);

    let operation_id = fixture.operation_id();
    let at = fixture.time();
    let expected_sources = [source, split]
        .into_iter()
        .map(|thought_id| {
            fixture
                .state
                .board
                .thought(thought_id)
                .expect("merge source")
                .clone()
        })
        .collect();
    reduce(
        &mut fixture.state,
        Action::MergeThoughts {
            thought_ids: vec![source, split],
            operation_id,
            expected_sources,
            separator: String::new(),
            at,
        },
    )
    .expect("merge");
    assert_name(&fixture, source, Some("Source label"));

    let extracted = fixture.ids.thought_id();
    let operation_id = fixture.operation_id();
    let at = fixture.time();
    reduce(
        &mut fixture.state,
        Action::ExtractThought {
            thought_id: source,
            new_thought_id: extracted,
            operation_id,
            expected_content: "left right".to_owned(),
            expected_annotations: Vec::new(),
            source_content: "left right".to_owned(),
            source_annotations: Vec::new(),
            range: 0..4,
            at,
        },
    )
    .expect("extract");
    assert_name(&fixture, source, Some("Source label"));
    assert_name(&fixture, extracted, None);
}

fn assert_name(fixture: &Fixture, thought_id: ThoughtId, expected: Option<&str>) {
    assert_eq!(
        fixture
            .state
            .board
            .thought(thought_id)
            .expect("thought")
            .name
            .as_ref()
            .map(ThoughtName::as_str),
        expected
    );
}
