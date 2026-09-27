use crate::{
    adapters::memory::FakeIdGenerator,
    application::text_reflow::TextReflowOutcome,
    domain::{BoardMutation, BoardOperationKind, ThoughtPosition},
    ports::{control::ControlMutation, environment::IdGenerator as _},
};

use super::super::matches_transform;
use super::support::{SeparatedFixture, at, digest, operation, separated_operation, stored};

#[test]
fn legacy_split_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_220_000_000);
    let session = ids.session_id();
    let source = ids.thought_id();
    let created = ids.thought_id();
    let operation_id = ids.operation_id();
    let stored = separated_operation(
        operation_id,
        session,
        source,
        created,
        &SeparatedFixture {
            kind: BoardOperationKind::Split,
            before: "leftRIGHT",
            retained: "left",
            separated: "RIGHT",
        },
    );
    let exact = ControlMutation::SplitThought {
        operation_id,
        thought_id: source,
        new_thought_id: created,
        expected_digest: digest("leftRIGHT"),
        at_byte: 4,
    };
    assert!(matches_transform(&stored, session, &exact));
    let drift = ControlMutation::SplitThought {
        operation_id,
        thought_id: source,
        new_thought_id: created,
        expected_digest: digest("leftRIGHT"),
        at_byte: 5,
    };
    assert!(!matches_transform(&stored, session, &drift));
}

#[test]
fn legacy_extract_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_221_000_000);
    let session = ids.session_id();
    let source = ids.thought_id();
    let created = ids.thought_id();
    let operation_id = ids.operation_id();
    let stored = separated_operation(
        operation_id,
        session,
        source,
        created,
        &SeparatedFixture {
            kind: BoardOperationKind::Extract,
            before: "leftMIDright",
            retained: "leftright",
            separated: "MID",
        },
    );
    let exact = ControlMutation::ExtractThought {
        operation_id,
        thought_id: source,
        new_thought_id: created,
        expected_digest: digest("leftMIDright"),
        start_byte: 4,
        end_byte: 7,
    };
    assert!(matches_transform(&stored, session, &exact));
    let drift = ControlMutation::ExtractThought {
        operation_id,
        thought_id: source,
        new_thought_id: created,
        expected_digest: digest("leftMIDright"),
        start_byte: 4,
        end_byte: 8,
    };
    assert!(!matches_transform(&stored, session, &drift));
}

#[test]
fn legacy_merge_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_222_000_000);
    let session = ids.session_id();
    let source = ids.thought_id();
    let second = ids.thought_id();
    let operation_id = ids.operation_id();
    let stored = stored(operation(
        operation_id,
        session,
        BoardOperationKind::Merge,
        BoardMutation::ReplaceContent {
            thought_id: source,
            before_content: "one".to_owned(),
            before_annotations: Vec::new(),
            after_content: "one\n\ntwo".to_owned(),
            after_annotations: Vec::new(),
        },
        BoardMutation::Batch {
            mutations: vec![
                BoardMutation::ReplaceContent {
                    thought_id: source,
                    before_content: "one\n\ntwo".to_owned(),
                    before_annotations: Vec::new(),
                    after_content: "one".to_owned(),
                    after_annotations: Vec::new(),
                },
                BoardMutation::SetDeletionExact {
                    thought_id: second,
                    expected_content: "two".to_owned(),
                    expected_annotations: Vec::new(),
                    expected_deleted_at: Some(at()),
                    expected_position: ThoughtPosition::new(1),
                    deleted_at: None,
                    position: ThoughtPosition::new(1),
                },
            ],
        },
    ));
    let exact = ControlMutation::MergeThoughts {
        operation_id,
        thought_ids: vec![source, second],
        expected_digests: vec![digest("one"), digest("two")],
        separator: "\n\n".to_owned(),
    };
    assert!(matches_transform(&stored, session, &exact));
    let drift = ControlMutation::MergeThoughts {
        operation_id,
        thought_ids: vec![source, second],
        expected_digests: vec![digest("one"), digest("two")],
        separator: "\n".to_owned(),
    };
    assert!(!matches_transform(&stored, session, &drift));
}

#[test]
fn legacy_reflow_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_223_000_000);
    let session = ids.session_id();
    let source = ids.thought_id();
    let operation_id = ids.operation_id();
    let before = "  hello   world  ";
    let projection = crate::application::text_reflow::reflow(before, &[]).expect("reflow");
    let TextReflowOutcome::Changed {
        content: after,
        annotations,
    } = projection.outcome
    else {
        panic!("changed reflow fixture")
    };
    let stored = stored(operation(
        operation_id,
        session,
        BoardOperationKind::Reflow,
        BoardMutation::ReplaceContent {
            thought_id: source,
            before_content: before.to_owned(),
            before_annotations: Vec::new(),
            after_content: after,
            after_annotations: annotations,
        },
        BoardMutation::Batch {
            mutations: Vec::new(),
        },
    ));
    let exact = ControlMutation::ReflowThought {
        operation_id,
        thought_id: source,
        expected_digest: digest(before),
    };
    assert!(matches_transform(&stored, session, &exact));
    let drift = ControlMutation::ReflowThought {
        operation_id,
        thought_id: source,
        expected_digest: digest("changed"),
    };
    assert!(!matches_transform(&stored, session, &drift));
}
