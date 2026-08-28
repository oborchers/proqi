//! Exact external edits routed through the same reducer histories as the TUI.

use sha2::{Digest as _, Sha256};

use crate::{
    application::{Action, reduce},
    domain::{OperationId, RevisionId, SessionId, TextPosition, ThoughtId},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::Store,
    },
};

use super::{SessionService, SessionServiceError, ThoughtMutation, match_replay};
use crate::application::{ControlReplay, match_control_replay};
use crate::ports::control::ControlMutation;

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Replace exact content as one persistent editor revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the session cannot be leased or loaded, the thought
    /// is missing, the digest is stale, or persistence fails.
    pub fn replace_thought(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        content: String,
        expected_digest: Option<[u8; 32]>,
        revision_id: RevisionId,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        let replay = ControlMutation::Replace {
            revision_id,
            thought_id,
            expected_digest,
            content: content.clone(),
        };
        if let Some(existing) = self.store.revision_request(revision_id)? {
            return match_existing_replacement(&existing, session_id, thought_id, &replay);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.revision_request(revision_id)? {
            return match_existing_replacement(&existing, session_id, thought_id, &replay);
        }
        let mut state = self.load_live_state(session_id)?;
        let thought = state
            .board
            .thought(thought_id)
            .filter(|thought| thought.is_live())
            .ok_or(crate::application::ApplicationError::ThoughtNotFound(
                thought_id,
            ))?
            .clone();
        let digest: [u8; 32] = Sha256::digest(thought.content.as_bytes()).into();
        if expected_digest.is_some_and(|expected| expected != digest) {
            return Err(crate::application::ApplicationError::ContentConflict(thought_id).into());
        }
        let effects = reduce(
            &mut state,
            Action::EditThought {
                thought_id,
                revision_id,
                before_content: thought.content.clone(),
                after_content: content,
                before_annotations: thought.annotations.clone(),
                after_annotations: Vec::new(),
                before_cursor: TextPosition::default(),
                after_cursor: TextPosition::default(),
                at: self.clock.now(),
            },
        )?;
        let receipt = self.commit_single_effect(&effects)?;
        Ok(ThoughtMutation {
            thought_id,
            receipt,
        })
    }

    /// Set one thought's compatibility collapsed state as a board operation.
    ///
    /// # Errors
    ///
    /// Returns an error when the session cannot be leased or loaded, the thought
    /// cannot be mutated, or persistence fails.
    pub fn set_thought_collapsed(
        &mut self,
        session_id: SessionId,
        thought_id: ThoughtId,
        collapsed: bool,
        operation_id: OperationId,
    ) -> Result<ThoughtMutation, SessionServiceError> {
        let replay = ControlMutation::SetCollapsed {
            operation_id,
            thought_id,
            collapsed,
        };
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_mutation(&existing, session_id, thought_id, &replay);
        }
        let _lease = self.runtime.acquire_session(session_id)?;
        if let Some(existing) = self.store.operation_request(operation_id)? {
            return match_existing_mutation(&existing, session_id, thought_id, &replay);
        }
        let mut state = self.load_live_state(session_id)?;
        let effects = reduce(
            &mut state,
            Action::SetPresentation {
                operation_id,
                thought_id,
                presentation: if collapsed {
                    crate::domain::ThoughtPresentation::Collapsed
                } else {
                    crate::domain::ThoughtPresentation::Automatic
                },
                at: self.clock.now(),
            },
        )?;
        let receipt = self.commit_single_effect(&effects)?;
        Ok(ThoughtMutation {
            thought_id,
            receipt,
        })
    }
}

fn match_existing_replacement(
    existing: &crate::ports::store::StoredOperationRequest,
    session_id: SessionId,
    thought_id: ThoughtId,
    mutation: &ControlMutation,
) -> Result<ThoughtMutation, SessionServiceError> {
    match match_control_replay(existing, session_id, mutation) {
        ControlReplay::Accepted(receipt) => Ok(ThoughtMutation {
            thought_id,
            receipt: receipt.durable,
        }),
        ControlReplay::Conflict => Err(crate::ports::store::StoreError::Conflict(
            "revision identity belongs to another replacement".to_owned(),
        )
        .into()),
    }
}

fn match_existing_mutation(
    existing: &crate::ports::store::StoredOperationRequest,
    session_id: SessionId,
    thought_id: ThoughtId,
    mutation: &ControlMutation,
) -> Result<ThoughtMutation, SessionServiceError> {
    let receipt = match_replay(existing, session_id, mutation)?;
    Ok(ThoughtMutation {
        thought_id,
        receipt: receipt.durable,
    })
}
