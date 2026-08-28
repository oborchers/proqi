use crate::{
    adapters::memory::FakeIdGenerator,
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, OperationSequence, Thought,
        ThoughtPosition, Timestamp, UndoScope,
    },
    ports::{
        control::ControlMutation,
        environment::IdGenerator,
        store::{CommitReceipt, DurableIdentity, StoredOperationRequest},
    },
};

use super::{ControlReplay, match_control_replay};

#[test]
fn exact_add_replay_is_accepted_but_changed_content_conflicts() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session_id = ids.session_id();
    let operation_id = ids.operation_id();
    let thought_id = ids.thought_id();
    let thought = Thought::new(
        thought_id,
        session_id,
        "exact".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let operation = BoardOperation {
        id: operation_id,
        session_id,
        sequence: crate::domain::OperationSequence::new(1),
        kind: BoardOperationKind::Create,
        forward: crate::domain::BoardMutation::AddThought {
            thought: thought.clone(),
        },
        inverse: crate::domain::BoardMutation::SetDeletion {
            thought_id,
            deleted_at: Some(Timestamp::from_millis(1)),
            position: thought.position,
        },
        created_at: Timestamp::from_millis(1),
    };
    let existing = StoredOperationRequest::Board {
        operation: Box::new(operation),
        receipt: CommitReceipt {
            session_id,
            sequence: crate::domain::OperationSequence::new(1),
            identity: DurableIdentity::Operation(operation_id),
            idempotent_replay: false,
        },
    };
    let exact = ControlMutation::Add {
        operation_id,
        thought_id,
        content: "exact".to_owned(),
        annotations: Vec::new(),
        position: Some(0),
    };
    assert!(matches!(
        match_control_replay(&existing, session_id, &exact),
        ControlReplay::Accepted(_)
    ));
    let changed = ControlMutation::Add {
        operation_id,
        thought_id,
        content: "changed".to_owned(),
        annotations: Vec::new(),
        position: Some(0),
    };
    assert!(matches!(
        match_control_replay(&existing, session_id, &changed),
        ControlReplay::Conflict
    ));
}

#[test]
fn delete_replay_requires_the_same_thought() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session_id = ids.session_id();
    let thought_id = ids.thought_id();
    let other_thought = ids.thought_id();
    let deleted_at = Timestamp::from_millis(2);
    let delete_id = ids.operation_id();
    let deletion = StoredOperationRequest::Board {
        operation: Box::new(BoardOperation {
            id: delete_id,
            session_id,
            sequence: OperationSequence::new(1),
            kind: BoardOperationKind::Delete,
            forward: BoardMutation::SetDeletion {
                thought_id,
                deleted_at: Some(deleted_at),
                position: ThoughtPosition::new(0),
            },
            inverse: BoardMutation::SetDeletion {
                thought_id,
                deleted_at: None,
                position: ThoughtPosition::new(0),
            },
            created_at: deleted_at,
        }),
        receipt: receipt(session_id, delete_id, 1),
    };
    let exact_delete = ControlMutation::Delete {
        operation_id: delete_id,
        thought_id,
    };
    assert_accepted(&deletion, session_id, &exact_delete);
    assert_conflict(
        &deletion,
        session_id,
        &ControlMutation::Delete {
            operation_id: delete_id,
            thought_id: other_thought,
        },
    );
}

#[test]
fn move_replay_requires_the_same_destination() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session_id = ids.session_id();
    let thought_id = ids.thought_id();
    let move_id = ids.operation_id();
    let movement = StoredOperationRequest::Board {
        operation: Box::new(BoardOperation {
            id: move_id,
            session_id,
            sequence: OperationSequence::new(2),
            kind: BoardOperationKind::Reorder,
            forward: BoardMutation::MoveThought {
                thought_id,
                from: ThoughtPosition::new(0),
                to: ThoughtPosition::new(1),
            },
            inverse: BoardMutation::MoveThought {
                thought_id,
                from: ThoughtPosition::new(1),
                to: ThoughtPosition::new(0),
            },
            created_at: Timestamp::from_millis(3),
        }),
        receipt: receipt(session_id, move_id, 2),
    };
    let exact_move = ControlMutation::Move {
        operation_id: move_id,
        thought_id,
        position: 1,
    };
    assert_accepted(&movement, session_id, &exact_move);
    let changed_move = ControlMutation::Move {
        operation_id: move_id,
        thought_id,
        position: 0,
    };
    assert_conflict(&movement, session_id, &changed_move);
}

#[test]
fn history_replay_requires_the_same_direction_and_scope() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session_id = ids.session_id();
    let history_id = ids.operation_id();
    let history = StoredOperationRequest::HistoryMove {
        session_id,
        scope: UndoScope::Board,
        undo: true,
        receipt: receipt(session_id, history_id, 3),
    };
    let exact_history = ControlMutation::History {
        operation_id: history_id,
        scope: UndoScope::Board,
        undo: true,
    };
    assert_accepted(&history, session_id, &exact_history);
    assert_conflict(
        &history,
        session_id,
        &ControlMutation::History {
            operation_id: history_id,
            scope: UndoScope::Board,
            undo: false,
        },
    );
}

fn receipt(
    session_id: crate::domain::SessionId,
    operation_id: crate::domain::OperationId,
    sequence: u64,
) -> CommitReceipt {
    CommitReceipt {
        session_id,
        sequence: OperationSequence::new(sequence),
        identity: DurableIdentity::Operation(operation_id),
        idempotent_replay: false,
    }
}

fn assert_accepted(
    existing: &StoredOperationRequest,
    session_id: crate::domain::SessionId,
    mutation: &ControlMutation,
) {
    assert!(matches!(
        match_control_replay(existing, session_id, mutation),
        ControlReplay::Accepted(_)
    ));
}

fn assert_conflict(
    existing: &StoredOperationRequest,
    session_id: crate::domain::SessionId,
    mutation: &ControlMutation,
) {
    assert!(matches!(
        match_control_replay(existing, session_id, mutation),
        ControlReplay::Conflict
    ));
}
