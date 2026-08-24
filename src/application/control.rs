//! Durable idempotency matching for owner-control requests.

use crate::{
    domain::{BoardMutation, BoardOperationKind, SessionId},
    ports::{
        control::{ControlMutation, ControlReceipt},
        store::{DurableIdentity, StoredOperationRequest},
    },
};

/// Result of comparing one requested operation with its durable identity.
pub(crate) enum ControlReplay {
    /// The exact mutation was already committed.
    Accepted(ControlReceipt),
    /// The operation identity belongs to another semantic request.
    Conflict,
}

/// Validate an operation replay without applying it to current state again.
pub(crate) fn match_control_replay(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> ControlReplay {
    let receipt = match existing {
        StoredOperationRequest::Board { receipt, .. }
        | StoredOperationRequest::HistoryMove { receipt, .. } => *receipt,
    };
    if receipt.identity != DurableIdentity::Operation(mutation.operation_id()) {
        return ControlReplay::Conflict;
    }
    let thought_id = match mutation {
        ControlMutation::Add { thought_id, .. } if matches_add(existing, session_id, mutation) => {
            Some(*thought_id)
        }
        ControlMutation::Delete { thought_id, .. }
            if matches_delete(existing, session_id, *thought_id) =>
        {
            Some(*thought_id)
        }
        ControlMutation::Move { thought_id, .. }
            if matches_move(existing, session_id, mutation) =>
        {
            Some(*thought_id)
        }
        ControlMutation::History { scope, undo, .. }
            if matches!(
                existing,
                StoredOperationRequest::HistoryMove {
                    session_id: stored_session,
                    scope: stored_scope,
                    undo: stored_undo,
                    ..
                } if *stored_session == session_id && stored_scope == scope && stored_undo == undo
            ) =>
        {
            None
        }
        _ => return ControlReplay::Conflict,
    };
    let mut durable = receipt;
    durable.idempotent_replay = true;
    ControlReplay::Accepted(ControlReceipt {
        thought_id,
        durable,
    })
}

fn matches_add(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let (
        StoredOperationRequest::Board { operation, .. },
        ControlMutation::Add {
            thought_id,
            content,
            annotations,
            position,
            ..
        },
    ) = (existing, mutation)
    else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Create
        && matches!(
            &operation.forward,
            BoardMutation::AddThought { thought }
                if thought.id == *thought_id
                    && thought.content == *content
                    && thought.annotations == *annotations
                    && position.is_none_or(|value| {
                        u32::try_from(value).ok() == Some(thought.position.get())
                    })
        )
}

fn matches_delete(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    thought_id: crate::domain::ThoughtId,
) -> bool {
    let StoredOperationRequest::Board { operation, .. } = existing else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Delete
        && matches!(
            &operation.forward,
            BoardMutation::SetDeletion {
                thought_id: stored,
                deleted_at: Some(_),
                ..
            } if *stored == thought_id
        )
}

fn matches_move(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let (
        StoredOperationRequest::Board { operation, .. },
        ControlMutation::Move {
            thought_id,
            position,
            ..
        },
    ) = (existing, mutation)
    else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Reorder
        && matches!(
            &operation.forward,
            BoardMutation::MoveThought {
                thought_id: stored,
                to,
                ..
            } if stored == thought_id && usize::try_from(to.get()).ok() == Some(*position)
        )
}

#[cfg(test)]
mod tests {
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
}
