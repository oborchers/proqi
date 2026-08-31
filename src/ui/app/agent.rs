//! Verified directional submission and non-destructive completion.

use crate::{
    application::{Action, Effect, reduce},
    domain::{BoardOperationKind, SubmissionId},
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
        self.set_info("checking adjacent agents");
        Self::discover_agents()
    }

    /// Replace targets only after complete verified discovery.
    pub fn complete_agent_discovery(&mut self, result: Result<Vec<AgentTarget>, AgentError>) {
        self.submission_mode = None;
        let was_refreshing = self.status_text() == Some("checking adjacent agents");
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
        let Some(pending) = self.pending_submissions.get_mut(&submission_id) else {
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
        let deletion_operation_id = (completion.is_ok()
            && pending.disposition == SubmissionDisposition::RemoveAfterSuccess)
            .then_some(pending.deletion_operation_id);
        let outcome = submission_outcome(&completion, deletion_operation_id, pending.at);
        pending.completion = Some(completion);
        vec![Effect::FinishSubmission {
            submission_id,
            outcome,
        }]
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
        let mut effects = vec![Effect::StoreIntegrationContext {
            session_id: self.state.board.session.id,
            target: receipt.target.clone(),
            verified_at: pending.at,
        }];
        let unchanged = pending.sources.iter().all(|source| {
            self.current_thought_digest(source.thought_id) == Some(source.source_digest)
        });
        if pending.disposition == SubmissionDisposition::RemoveAfterSuccess && unchanged {
            effects.extend(
                self.reduce(Action::DeleteThoughts {
                    operation_id: pending.deletion_operation_id,
                    thought_ids: pending
                        .sources
                        .iter()
                        .map(|source| source.thought_id)
                        .collect(),
                    kind: BoardOperationKind::SubmitAndRemove,
                    at: pending.at,
                }),
            );
            self.clear_board_selection();
        }
        let multiple = pending.sources.len() > 1;
        let outcome = match (pending.disposition, unchanged, multiple) {
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
        self.set_success(format!(
            "submitted {} to {}, {outcome}",
            receipt.target.direction.as_str(),
            receipt.target.agent_name
        ));
        if receipt.target.agent_session.is_provisional() {
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
