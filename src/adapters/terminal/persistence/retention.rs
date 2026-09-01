//! Bounded accounting for exact failed persistence batches.

use std::collections::BTreeMap;

use crate::{domain::OperationSequence, ports::store::OperationBatch};

use super::RetainedCommit;

const RETAINED_BATCH_LIMIT: usize = 128;
const RETAINED_BYTES_LIMIT: usize = 16 * 1024 * 1024;

pub(super) fn can_retain(
    retained: &BTreeMap<OperationSequence, RetainedCommit>,
    sequence: OperationSequence,
    commit: &RetainedCommit,
) -> bool {
    let replacing = retained.contains_key(&sequence);
    let count = retained.len() + usize::from(!replacing);
    if count > RETAINED_BATCH_LIMIT {
        return false;
    }
    let previous = retained.get(&sequence).map_or(0, retained_commit_bytes);
    let bytes = retained
        .values()
        .map(retained_commit_bytes)
        .sum::<usize>()
        .saturating_sub(previous)
        .saturating_add(retained_commit_bytes(commit));
    bytes <= RETAINED_BYTES_LIMIT
}

fn retained_commit_bytes(commit: &RetainedCommit) -> usize {
    match commit {
        RetainedCommit::Batch(batch) => retained_batch_bytes(batch),
        RetainedCommit::SubmissionRemoval {
            outcome, removal, ..
        } => serde_json::to_vec(removal)
            .map_or(usize::MAX, |value| value.len())
            .saturating_add(
                outcome
                    .error_code
                    .as_ref()
                    .map_or(256, |code| code.len() + 256),
            ),
    }
}

fn retained_batch_bytes(batch: &crate::ports::store::OperationBatch) -> usize {
    match batch {
        OperationBatch::Board(operation) => {
            serde_json::to_vec(operation).map_or(usize::MAX, |value| value.len())
        }
        OperationBatch::Revision(revision) => {
            serde_json::to_vec(revision).map_or(usize::MAX, |value| value.len())
        }
        OperationBatch::HistoryMove { .. } => 256,
        OperationBatch::CreateSession(session) => session
            .name
            .as_ref()
            .map_or(256, |name| name.len().saturating_add(256)),
        OperationBatch::IntegrationContext { context, .. } => context
            .as_ref()
            .and_then(|value| serde_json::to_vec(value).ok())
            .map_or(256, |value| value.len()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{
            BoardMutation, BoardOperation, BoardOperationKind, OperationSequence, Thought,
            ThoughtPosition, Timestamp,
        },
        ports::{environment::IdGenerator as _, store::OperationBatch},
    };

    use super::{RetainedCommit, can_retain};

    #[test]
    fn oversized_failed_batch_is_not_offered_for_retry() {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session_id = ids.session_id();
        let thought = Thought::new(
            ids.thought_id(),
            session_id,
            "x".repeat(17 * 1024 * 1024),
            ThoughtPosition::new(0),
            Timestamp::from_millis(1),
        );
        let batch = OperationBatch::Board(BoardOperation {
            id: ids.operation_id(),
            session_id,
            sequence: OperationSequence::new(1),
            kind: BoardOperationKind::Create,
            forward: BoardMutation::AddThought {
                thought: thought.clone(),
            },
            inverse: BoardMutation::SetDeletion {
                thought_id: thought.id,
                deleted_at: Some(Timestamp::from_millis(1)),
                position: thought.position,
            },
            created_at: Timestamp::from_millis(1),
        });
        assert!(!can_retain(
            &BTreeMap::new(),
            OperationSequence::new(1),
            &RetainedCommit::Batch(Box::new(batch))
        ));
    }
}
