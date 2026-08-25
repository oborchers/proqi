//! Content-redacted request identity retained after history compaction.

use serde::{Deserialize, Serialize};

use crate::domain::{ContentAnnotation, SessionId, ThoughtId, UndoScope};

use super::StoreError;

/// Minimal content-redacted request identity retained after history compaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CompactedOperationRequest {
    /// Thought creation with a digest of exact content and annotations.
    Add {
        /// Owning session.
        session_id: SessionId,
        /// Created thought.
        thought_id: ThoughtId,
        /// SHA-256 of canonical content and annotations.
        payload_digest: [u8; 32],
        /// Durable insertion position.
        position: usize,
    },
    /// Thought deletion, cut, or accepted submit-and-remove.
    Delete {
        /// Owning session.
        session_id: SessionId,
        /// Deleted thought.
        thought_id: ThoughtId,
    },
    /// Thought reordering.
    Move {
        /// Owning session.
        session_id: SessionId,
        /// Moved thought.
        thought_id: ThoughtId,
        /// Durable destination position.
        position: usize,
    },
    /// Persistent board or editor undo and redo.
    History {
        /// Owning session.
        session_id: SessionId,
        /// Addressed history scope.
        scope: UndoScope,
        /// Undo when true, redo when false.
        undo: bool,
    },
    /// A durable operation that has no public replay contract.
    Opaque,
}

/// Hash exact thought content and presentation annotations for redacted replay matching.
///
/// # Errors
///
/// Returns a serialization error when annotations cannot be encoded canonically.
pub fn thought_payload_digest(
    content: &str,
    annotations: &[ContentAnnotation],
) -> Result<[u8; 32], StoreError> {
    use sha2::{Digest as _, Sha256};

    let annotations = serde_json::to_vec(annotations)
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(
        u64::try_from(content.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    digest.update(content.as_bytes());
    digest.update(annotations);
    Ok(digest.finalize().into())
}
