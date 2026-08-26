//! Exact idempotency comparison for editor replacement revisions.

use sha2::Digest as _;

use crate::{
    domain::SessionId,
    ports::{control::ControlMutation, store::StoredOperationRequest},
};

pub(super) fn matches(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let (
        StoredOperationRequest::Revision { revision, .. },
        ControlMutation::Replace {
            thought_id,
            expected_digest,
            content,
            ..
        },
    ) = (existing, mutation)
    else {
        return false;
    };
    let before_digest: [u8; 32] = sha2::Sha256::digest(revision.before_content.as_bytes()).into();
    revision.session_id == session_id
        && revision.thought_id == *thought_id
        && revision.after_content == *content
        && expected_digest.is_none_or(|expected| expected == before_digest)
}

#[cfg(test)]
mod tests {
    use sha2::Digest as _;

    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{OperationSequence, TextPosition, ThoughtRevision, Timestamp},
        ports::{
            control::ControlMutation,
            environment::IdGenerator,
            store::{CommitReceipt, DurableIdentity, StoredOperationRequest},
        },
    };

    #[test]
    fn replacement_replay_requires_exact_content_and_precondition() {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session_id = ids.session_id();
        let thought_id = ids.thought_id();
        let revision_id = ids.revision_id();
        let sequence = OperationSequence::new(2);
        let revision = ThoughtRevision {
            id: revision_id,
            session_id,
            thought_id,
            sequence,
            before_content: "before".to_owned(),
            after_content: "after".to_owned(),
            before_annotations: Vec::new(),
            after_annotations: Vec::new(),
            before_cursor: TextPosition::default(),
            after_cursor: TextPosition::default(),
            created_at: Timestamp::from_millis(2),
        };
        let existing = StoredOperationRequest::Revision {
            revision: Box::new(revision),
            receipt: CommitReceipt {
                session_id,
                sequence,
                identity: DurableIdentity::Revision(revision_id),
                idempotent_replay: true,
            },
        };
        let digest: [u8; 32] = sha2::Sha256::digest(b"before").into();
        let exact = ControlMutation::Replace {
            revision_id,
            thought_id,
            expected_digest: Some(digest),
            content: "after".to_owned(),
        };
        assert!(super::matches(&existing, session_id, &exact));
        let changed = ControlMutation::Replace {
            revision_id,
            thought_id,
            expected_digest: Some(digest),
            content: "changed".to_owned(),
        };
        assert!(!super::matches(&existing, session_id, &changed));
    }
}
