use crate::{
    adapters::memory::FakeIdGenerator,
    application::derived_duplicate_item_ids,
    domain::{BoardItemId, BoardMutation, BoardOperationKind, Separator, Thought, ThoughtPosition},
    ports::{control::ControlMutation, environment::IdGenerator as _},
};

use super::super::{
    matches_delete_items, matches_duplicate_items, matches_insert_separator, matches_move_item,
};
use super::support::{at, operation, stored};

#[test]
fn legacy_insert_separator_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_210_000_000);
    let session = ids.session_id();
    let operation_id = ids.operation_id();
    let separator_id = ids.separator_id();
    let stored = stored(operation(
        operation_id,
        session,
        BoardOperationKind::InsertSeparator,
        BoardMutation::AddSeparator {
            separator: Separator::new(separator_id, session, ThoughtPosition::new(0), at()),
        },
        BoardMutation::SetSeparatorDeletion {
            separator_id,
            deleted_at: Some(at()),
            position: ThoughtPosition::new(0),
        },
    ));
    let exact = ControlMutation::InsertSeparator {
        operation_id,
        separator_id,
        position: Some(0),
    };
    assert!(matches_insert_separator(&stored, session, &exact));
    let drift = ControlMutation::InsertSeparator {
        operation_id,
        separator_id,
        position: Some(1),
    };
    assert!(!matches_insert_separator(&stored, session, &drift));
}

#[test]
fn legacy_move_item_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_211_000_000);
    let session = ids.session_id();
    let operation_id = ids.operation_id();
    let separator_id = ids.separator_id();
    let stored = stored(operation(
        operation_id,
        session,
        BoardOperationKind::Reorder,
        BoardMutation::MoveSeparator {
            separator_id,
            from: ThoughtPosition::new(0),
            to: ThoughtPosition::new(1),
        },
        BoardMutation::MoveSeparator {
            separator_id,
            from: ThoughtPosition::new(1),
            to: ThoughtPosition::new(0),
        },
    ));
    let exact = ControlMutation::MoveItem {
        operation_id,
        item_id: BoardItemId::Separator(separator_id),
        position: 1,
    };
    assert!(matches_move_item(&stored, session, &exact));
    let drift = ControlMutation::MoveItem {
        operation_id,
        item_id: BoardItemId::Separator(separator_id),
        position: 0,
    };
    assert!(!matches_move_item(&stored, session, &drift));
}

#[test]
fn legacy_delete_items_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_212_000_000);
    let session = ids.session_id();
    let operation_id = ids.operation_id();
    let thought_id = ids.thought_id();
    let separator_id = ids.separator_id();
    let item_ids = vec![thought_id.into(), separator_id.into()];
    let stored = stored(operation(
        operation_id,
        session,
        BoardOperationKind::Delete,
        BoardMutation::Batch {
            mutations: Vec::new(),
        },
        BoardMutation::Batch {
            mutations: vec![
                BoardMutation::SetDeletion {
                    thought_id,
                    deleted_at: None,
                    position: ThoughtPosition::new(0),
                },
                BoardMutation::SetSeparatorDeletion {
                    separator_id,
                    deleted_at: None,
                    position: ThoughtPosition::new(1),
                },
            ],
        },
    ));
    let exact = ControlMutation::DeleteItems {
        operation_id,
        item_ids: item_ids.clone(),
    };
    assert!(matches_delete_items(&stored, session, &exact));
    let drift = ControlMutation::DeleteItems {
        operation_id,
        item_ids: item_ids.into_iter().rev().collect(),
    };
    assert!(!matches_delete_items(&stored, session, &drift));
}

#[test]
fn legacy_duplicate_items_matcher_accepts_exact_shape_and_rejects_drift() {
    let mut ids = FakeIdGenerator::new(1_725_213_000_000);
    let session = ids.session_id();
    let operation_id = ids.operation_id();
    let thought_id = ids.thought_id();
    let separator_id = ids.separator_id();
    let item_ids = vec![thought_id.into(), separator_id.into()];
    let outputs = derived_duplicate_item_ids(operation_id, &item_ids).expect("duplicate IDs");
    let BoardItemId::Thought(output_thought) = outputs[0] else {
        panic!("thought output")
    };
    let BoardItemId::Separator(output_separator) = outputs[1] else {
        panic!("separator output")
    };
    let stored = stored(operation(
        operation_id,
        session,
        BoardOperationKind::Duplicate,
        BoardMutation::Batch {
            mutations: vec![
                BoardMutation::AddThought {
                    thought: Thought::new(
                        output_thought,
                        session,
                        "copy".to_owned(),
                        ThoughtPosition::new(2),
                        at(),
                    ),
                },
                BoardMutation::AddSeparator {
                    separator: Separator::new(
                        output_separator,
                        session,
                        ThoughtPosition::new(3),
                        at(),
                    ),
                },
            ],
        },
        BoardMutation::Batch {
            mutations: Vec::new(),
        },
    ));
    let exact = ControlMutation::DuplicateItems {
        operation_id,
        item_ids: item_ids.clone(),
        duplicate_ids: Vec::new(),
    };
    assert!(matches_duplicate_items(&stored, session, &exact));
    let drift = ControlMutation::DuplicateItems {
        operation_id,
        item_ids: item_ids.into_iter().rev().collect(),
        duplicate_ids: Vec::new(),
    };
    assert!(!matches_duplicate_items(&stored, session, &drift));
}
