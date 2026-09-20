//! Canonical preconditions shared by inactive services and active owners.

use sha2::{Digest as _, Sha256};

use super::{AppState, ApplicationError, ApplicationResult};
use crate::domain::{Thought, ThoughtId};

/// Resolve one live thought and optionally require its exact content digest.
pub(crate) fn exact_live_thought(
    state: &AppState,
    thought_id: ThoughtId,
    expected_digest: Option<[u8; 32]>,
) -> ApplicationResult<&Thought> {
    let thought = state
        .board
        .thought(thought_id)
        .filter(|thought| thought.is_live())
        .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
    let digest: [u8; 32] = Sha256::digest(thought.content.as_bytes()).into();
    if expected_digest.is_some_and(|expected| expected != digest) {
        return Err(ApplicationError::ContentConflict(thought_id));
    }
    Ok(thought)
}
