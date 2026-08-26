//! Exact external edits routed through the same reducer histories as the TUI.

use sha2::{Digest as _, Sha256};

use crate::{
    application::{Action, AppState, Effect, reduce},
    domain::{OperationId, RevisionId, SessionId, TextPosition, ThoughtId},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{CommitReceipt, Store},
    },
};

use super::{SessionService, SessionServiceError, ThoughtMutation};

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
        let _lease = self.runtime.acquire_session(session_id)?;
        let mut state = self.load_external_state(session_id)?;
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
        let receipt = self.commit_external(&effects)?;
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
        let _lease = self.runtime.acquire_session(session_id)?;
        let mut state = self.load_external_state(session_id)?;
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
        let receipt = self.commit_external(&effects)?;
        Ok(ThoughtMutation {
            thought_id,
            receipt,
        })
    }

    fn load_external_state(&mut self, id: SessionId) -> Result<AppState, SessionServiceError> {
        self.store.compact_session(id)?;
        let snapshot = self.store.load_session(id)?;
        if snapshot.board.session.deleted_at.is_some() {
            return Err(SessionServiceError::SessionTrashed(id));
        }
        Ok(AppState::from_snapshot(snapshot)?)
    }

    fn commit_external(
        &mut self,
        effects: &[Effect],
    ) -> Result<CommitReceipt, SessionServiceError> {
        let [effect] = effects else {
            return Err(SessionServiceError::NoDurableMutation);
        };
        let batch = effect
            .persistence_batch()
            .ok_or(SessionServiceError::NoDurableMutation)?;
        self.store
            .commit(&batch)?
            .ok_or(SessionServiceError::NoDurableMutation)
    }
}
