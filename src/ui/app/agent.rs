//! Verified directional submission and non-destructive completion.

use crate::{
    application::{Action, Effect, reduce},
    domain::SubmissionId,
    ports::{
        agent::{AgentError, AgentTarget, SubmissionDisposition, SubmissionReceipt},
        store::{StoreError, SubmissionAttemptState, SubmissionOutcome},
    },
};
use sha2::{Digest, Sha256};

use super::{BoardApp, pending_types::PendingSubmission};

impl BoardApp {
    /// Initial optional-integration discovery effect.
    #[must_use]
    pub fn discover_agents() -> Vec<Effect> {
        vec![Effect::DiscoverAgents]
    }

    /// Request explicit rediscovery and make its result visible to the user.
    pub(super) fn refresh_agents(&mut self) -> Vec<Effect> {
        self.agent_refresh_in_flight = true;
        self.set_info("checking adjacent agents");
        Self::discover_agents()
    }

    /// Replace targets only after complete verified discovery.
    pub fn complete_agent_discovery(&mut self, result: Result<Vec<AgentTarget>, AgentError>) {
        self.submission_mode = None;
        let was_refreshing = std::mem::take(&mut self.agent_refresh_in_flight);
        match result {
            Ok(targets) => {
                if was_refreshing {
                    self.set_success(discovery_status(targets.len()));
                }
                self.agent_targets = targets;
            }
            Err(error @ (AgentError::Unavailable(_) | AgentError::Unsupported(_))) => {
                self.agent_targets.clear();
                if was_refreshing {
                    self.set_warning(format!("direct submission unavailable: {error}"));
                }
            }
            Err(error) if was_refreshing => {
                self.agent_targets.clear();
                self.set_error(format!("direct submission unavailable: {error}"));
            }
            Err(_) => self.agent_targets.clear(),
        }
        self.refresh_invocation_popup();
    }

    /// Persist the prepared intent before external delivery begins.
    pub fn complete_submission_prepared(
        &mut self,
        submission_id: SubmissionId,
        result: Result<(), StoreError>,
    ) -> Vec<Effect> {
        let Some(pending) = self.pending_submissions.get(&submission_id) else {
            return Vec::new();
        };
        if let Err(error) = result {
            let sources = pending_source_ids(pending);
            let kept = kept_sentence(sources.len());
            self.pending_submissions.remove(&submission_id);
            self.release_submission_sources(sources);
            self.set_error(format!("Submission not started. {kept}. {error}"));
            return Vec::new();
        }
        vec![Effect::MarkSubmissionSending {
            submission_id,
            at: pending.at,
        }]
    }

    /// Submit only after the sending transition is durable.
    pub fn complete_submission_sending(
        &mut self,
        submission_id: SubmissionId,
        result: Result<(), StoreError>,
    ) -> Vec<Effect> {
        let Some(pending) = self.pending_submissions.get(&submission_id) else {
            return Vec::new();
        };
        if let Err(error) = result {
            let sources = pending_source_ids(pending);
            let kept = kept_sentence(sources.len());
            self.pending_submissions.remove(&submission_id);
            self.release_submission_sources(sources);
            self.set_error(format!("Submission not started. {kept}. {error}"));
            return Vec::new();
        }
        vec![Effect::SubmitAgent(pending.request.clone())]
    }

    /// Stage one external result for a durable terminal journal transition.
    pub fn complete_submission(
        &mut self,
        submission_id: SubmissionId,
        result: Result<SubmissionReceipt, AgentError>,
    ) -> Vec<Effect> {
        let (completion, disposition, deletion_operation_id, at, sources) = {
            let Some(pending) = self.pending_submissions.get(&submission_id) else {
                return Vec::new();
            };
            let completion = match result {
                Ok(receipt)
                    if receipt.submission_id == submission_id
                        && pending.request.target.accepts_receipt(&receipt.target) =>
                {
                    Ok(receipt)
                }
                Ok(_) => Err(AgentError::Malformed(
                    "prompt receipt did not match the request".to_owned(),
                )),
                Err(error) => Err(error),
            };
            let sources = pending
                .sources
                .iter()
                .map(|source| (source.thought_id, source.source_digest))
                .collect::<Vec<_>>();
            (
                completion,
                pending.disposition,
                pending.deletion_operation_id,
                pending.at,
                sources,
            )
        };
        let should_remove = completion.is_ok()
            && disposition == SubmissionDisposition::RemoveAfterSuccess
            && sources.iter().all(|(thought_id, source_digest)| {
                self.current_thought_digest(*thought_id) == Some(*source_digest)
            });
        let removal = should_remove
            .then(|| {
                self.reduce(Action::StageSubmissionRemoval {
                    operation_id: deletion_operation_id,
                    thought_ids: sources.iter().map(|(thought_id, _)| *thought_id).collect(),
                    at,
                })
            })
            .and_then(|effects| {
                effects.into_iter().find_map(|effect| match effect {
                    Effect::CommitBoardOperation(operation) => Some(operation),
                    _ => None,
                })
            });
        let deletion_operation_id = removal.as_ref().map(|operation| operation.id);
        let removal_sequence = removal.as_ref().map(|operation| operation.sequence);
        let outcome = submission_outcome(&completion, deletion_operation_id, at);
        let Some(pending) = self.pending_submissions.get_mut(&submission_id) else {
            return Vec::new();
        };
        pending.completion = Some(completion);
        pending.removal_sequence = removal_sequence;
        vec![Effect::FinishSubmission {
            submission_id,
            outcome,
            removal,
        }]
    }

    /// Keep an accepted draft and its lock while its atomic journal/removal commit is retryable.
    pub fn submission_persistence_failed(
        &mut self,
        submission_id: SubmissionId,
        error: &StoreError,
    ) {
        let Some(pending) = self.pending_submissions.get(&submission_id) else {
            return;
        };
        let kept = kept_sentence(pending.sources.len());
        let recovery = if *error == StoreError::RecoveryCapacity {
            "press w to export recovery"
        } else {
            "press r to retry or w to export recovery"
        };
        self.set_error(format!(
            "Submission accepted, but its outcome and removal were not saved. {kept}; {recovery}. {error}"
        ));
    }

    /// Apply user-visible completion only after the journal terminal state is durable.
    pub fn complete_submission_journaled(
        &mut self,
        submission_id: SubmissionId,
        result: Result<(), StoreError>,
    ) -> Vec<Effect> {
        let Some(mut pending) = self.pending_submissions.remove(&submission_id) else {
            return Vec::new();
        };
        self.release_submission_sources(pending_source_ids(&pending));
        let Some(completion) = pending.completion.take() else {
            return Vec::new();
        };
        if let Err(error) = result {
            let status = if completion.is_ok() {
                "accepted"
            } else {
                "failed"
            };
            let kept = kept_sentence(pending.sources.len());
            self.set_error(format!(
                "Submission {status}, but its outcome was not saved. {kept}. {error}"
            ));
            return Vec::new();
        }
        match completion {
            Ok(receipt) => self.apply_accepted_submission(&pending, &receipt),
            Err(error) => {
                let kept = kept_sentence(pending.sources.len());
                self.set_error(format!("Submission failed. {kept}. {error}"));
                Vec::new()
            }
        }
    }

    pub(super) fn release_submission_sources(
        &mut self,
        thought_ids: Vec<crate::domain::ThoughtId>,
    ) {
        if let Err(error) = reduce(&mut self.state, Action::EndSubmission { thought_ids }) {
            self.set_error(error.to_string());
        }
    }

    fn apply_accepted_submission(
        &mut self,
        pending: &PendingSubmission,
        receipt: &SubmissionReceipt,
    ) -> Vec<Effect> {
        if let Some(target) = self
            .agent_targets
            .iter_mut()
            .find(|target| target.identity() == pending.request.target.identity())
        {
            *target = receipt.target.clone();
        }
        let mut effects = receipt
            .target
            .adjacent_direction()
            .map(|_| Effect::StoreIntegrationContext {
                session_id: self.state.board.session.id,
                target: receipt.target.clone(),
                verified_at: pending.at,
            })
            .into_iter()
            .collect::<Vec<_>>();
        let removed = pending.removal_sequence.is_some();
        if removed {
            self.state.reconcile_empty_board(
                crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
            );
            if matches!(
                self.state.mode,
                crate::application::InteractionMode::Compose
            ) {
                self.compose_presentation = super::ComposePresentation::Prompt;
            }
            self.sync_editor_from_state();
            self.clear_board_selection();
        }
        let multiple = pending.sources.len() > 1;
        let outcome = match (pending.disposition, removed, multiple) {
            (SubmissionDisposition::Keep, _, false) => "thought kept",
            (SubmissionDisposition::Keep, _, true) => "thoughts kept",
            (SubmissionDisposition::RemoveAfterSuccess, true, false) => "thought removed",
            (SubmissionDisposition::RemoveAfterSuccess, true, true) => "thoughts removed",
            (SubmissionDisposition::RemoveAfterSuccess, false, false) => {
                "thought changed during submission and was kept"
            }
            (SubmissionDisposition::RemoveAfterSuccess, false, true) => {
                "thoughts changed during submission and were kept"
            }
        };
        let destination = receipt.target.adjacent_direction().map_or_else(
            || format!("to {}", receipt.target.agent_name),
            |direction| format!("{} to {}", direction.as_str(), receipt.target.agent_name),
        );
        self.set_success(format!("submitted {destination}, {outcome}"));
        if receipt.target.adjacent_direction().is_some()
            && receipt.target.agent_session().is_provisional()
        {
            effects.push(Effect::DiscoverAgents);
        }
        effects
    }

    pub(super) fn current_thought_digest(
        &self,
        thought_id: crate::domain::ThoughtId,
    ) -> Option<[u8; 32]> {
        if let Some((pending_id, snapshot)) = self.pending_edit_snapshot()
            && pending_id == thought_id
        {
            return Some(digest(snapshot.content.as_bytes()));
        }
        self.state
            .board
            .thought(thought_id)
            .filter(|thought| thought.is_live())
            .map(|thought| digest(thought.content.as_bytes()))
    }
}

pub(super) fn pending_source_ids(pending: &PendingSubmission) -> Vec<crate::domain::ThoughtId> {
    pending
        .sources
        .iter()
        .map(|source| source.thought_id)
        .collect()
}

pub(super) fn kept_sentence(source_count: usize) -> &'static str {
    if source_count == 1 {
        "Thought kept"
    } else {
        "Thoughts kept"
    }
}

fn submission_outcome(
    result: &Result<SubmissionReceipt, AgentError>,
    deletion_operation_id: Option<crate::domain::OperationId>,
    at: crate::domain::Timestamp,
) -> SubmissionOutcome {
    match result {
        Ok(receipt) => SubmissionOutcome {
            state: SubmissionAttemptState::Accepted,
            post_state: receipt.post_state,
            error_code: None,
            deletion_operation_id,
            at,
        },
        Err(error) => SubmissionOutcome {
            state: SubmissionAttemptState::Failed,
            post_state: None,
            error_code: Some(agent_error_code(error).to_owned()),
            deletion_operation_id: None,
            at,
        },
    }
}

fn agent_error_code(error: &AgentError) -> &'static str {
    error.stable_code().as_str()
}

pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn discovery_status(target_count: usize) -> String {
    match target_count {
        0 => "no verified adjacent agent".to_owned(),
        1 => "verified 1 adjacent agent".to_owned(),
        count => format!("verified {count} adjacent agents"),
    }
}
