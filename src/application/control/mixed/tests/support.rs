use sha2::{Digest as _, Sha256};

use crate::{
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, OperationId, OperationSequence,
        SessionId, Thought, ThoughtId, ThoughtPosition, Timestamp,
    },
    ports::store::{CommitReceipt, DurableIdentity, StoredOperationRequest},
};

pub(super) struct SeparatedFixture<'a> {
    pub(super) kind: BoardOperationKind,
    pub(super) before: &'a str,
    pub(super) retained: &'a str,
    pub(super) separated: &'a str,
}

pub(super) fn separated_operation(
    id: OperationId,
    session: SessionId,
    source: ThoughtId,
    created: ThoughtId,
    fixture: &SeparatedFixture<'_>,
) -> StoredOperationRequest {
    stored(operation(
        id,
        session,
        fixture.kind,
        BoardMutation::Batch {
            mutations: vec![
                BoardMutation::ReplaceContent {
                    thought_id: source,
                    before_content: fixture.before.to_owned(),
                    before_annotations: Vec::new(),
                    after_content: fixture.retained.to_owned(),
                    after_annotations: Vec::new(),
                },
                BoardMutation::AddThought {
                    thought: Thought::new(
                        created,
                        session,
                        fixture.separated.to_owned(),
                        ThoughtPosition::new(1),
                        at(),
                    ),
                },
            ],
        },
        BoardMutation::Batch {
            mutations: Vec::new(),
        },
    ))
}

pub(super) fn operation(
    id: OperationId,
    session_id: SessionId,
    kind: BoardOperationKind,
    forward: BoardMutation,
    inverse: BoardMutation,
) -> BoardOperation {
    BoardOperation {
        id,
        session_id,
        sequence: OperationSequence::new(1),
        kind,
        forward,
        inverse,
        created_at: at(),
    }
}

pub(super) fn stored(operation: BoardOperation) -> StoredOperationRequest {
    let receipt = CommitReceipt {
        session_id: operation.session_id,
        sequence: operation.sequence,
        identity: DurableIdentity::Operation(operation.id),
        idempotent_replay: false,
    };
    StoredOperationRequest::Board {
        operation: Box::new(operation),
        semantic_fingerprint: None,
        receipt,
    }
}

pub(super) fn at() -> Timestamp {
    Timestamp::from_millis(10)
}

pub(super) fn digest(content: &str) -> [u8; 32] {
    Sha256::digest(content.as_bytes()).into()
}
