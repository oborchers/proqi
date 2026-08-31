//! Durability-gated construction and release of adjacent-agent submissions.

use crate::{
    application::{Action, DurabilityState, Effect, FailureCode, reduce},
    domain::{Thought, ThoughtId},
    ports::{
        agent::{AgentTarget, SubmissionDisposition, SubmissionRequest},
        environment::{Clock, IdGenerator},
        store::{SubmissionAttempt, SubmissionSource},
    },
};

use super::{
    BoardApp,
    agent::{digest, kept_sentence, pending_source_ids},
    agent_identity::target_fingerprint,
    pending_types::{DeferredSubmissionIntent, PendingSubmission, PendingSubmissionSource},
};

impl BoardApp {
    pub(super) fn queue_submission(
        &mut self,
        target: &AgentTarget,
        disposition: SubmissionDisposition,
        thought_ids: &[ThoughtId],
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            self.set_warning("save changes before submitting");
            return Vec::new();
        }
        let source_thoughts = thought_ids
            .iter()
            .filter_map(|id| self.state.board.thought(*id))
            .filter(|thought| thought.is_live())
            .cloned()
            .collect::<Vec<_>>();
        if source_thoughts.len() != thought_ids.len() {
            self.set_warning("board changed before submission; thoughts kept");
            return Vec::new();
        }
        let intent =
            self.build_submission_intent(target, disposition, &source_thoughts, ids, clock);
        let source_ids = intent
            .pending
            .sources
            .iter()
            .map(|source| source.thought_id)
            .collect();
        if let Err(error) = reduce(
            &mut self.state,
            Action::BeginSubmission {
                thought_ids: source_ids,
            },
        ) {
            self.set_error(error.to_string());
            return Vec::new();
        }
        self.start_or_defer_submission(intent)
    }

    fn build_submission_intent(
        &self,
        target: &AgentTarget,
        disposition: SubmissionDisposition,
        source_thoughts: &[Thought],
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> DeferredSubmissionIntent {
        let source_contents = source_thoughts
            .iter()
            .map(|thought| (thought.id, thought.content.clone()))
            .collect::<Vec<_>>();
        let content = crate::application::join_prompt_for_target(target, &source_contents);
        let submission_id = ids.submission_id();
        let payload_digest = digest(content.as_bytes());
        let at = clock.now();
        let sources = source_contents
            .iter()
            .map(|(thought_id, content)| PendingSubmissionSource {
                thought_id: *thought_id,
                source_digest: digest(content.as_bytes()),
            })
            .collect();
        let request = SubmissionRequest {
            submission_id,
            target: target.clone(),
            content,
        };
        let pending = PendingSubmission {
            request: request.clone(),
            sources,
            at,
            disposition,
            deletion_operation_id: ids.operation_id(),
            completion: None,
        };
        let attempt = SubmissionAttempt {
            id: submission_id,
            session_id: self.state.board.session.id,
            sources: source_contents
                .into_iter()
                .map(|(thought_id, content)| SubmissionSource {
                    thought_id,
                    source_digest: digest(content.as_bytes()),
                })
                .collect(),
            payload_digest,
            source_sequence: self.state.board.session.last_durable_sequence,
            disposition,
            direction: target.direction,
            provider: target.provider.clone(),
            protocol: target.protocol,
            target_fingerprint: target_fingerprint(target),
            pre_state: target.readiness,
            prepared_at: at,
        };
        let attachment_keys = source_thoughts
            .iter()
            .flat_map(crate::application::attachment_keys)
            .collect();
        DeferredSubmissionIntent {
            attempt,
            pending,
            attachment_keys,
        }
    }

    fn start_or_defer_submission(&mut self, intent: DeferredSubmissionIntent) -> Vec<Effect> {
        let submission_id = intent.attempt.id;
        if matches!(self.state.durability, DurabilityState::Durable { .. }) {
            self.begin_submission_preflight(intent)
        } else {
            self.deferred_submissions.insert(submission_id, intent);
            self.set_info("saving changes before submission");
            Vec::new()
        }
    }

    pub(super) fn complete_deferred_submission_durability(
        &mut self,
        failure: Option<FailureCode>,
    ) -> Vec<Effect> {
        if let Some(code) = failure {
            self.cancel_deferred_submissions(code);
            return Vec::new();
        }
        if !matches!(self.state.durability, DurabilityState::Durable { .. }) {
            return Vec::new();
        }
        let deferred = std::mem::take(&mut self.deferred_submissions);
        let mut effects = Vec::new();
        for (_submission_id, mut intent) in deferred {
            intent.attempt.source_sequence = self.state.board.session.last_durable_sequence;
            effects.extend(self.begin_submission_preflight(intent));
        }
        effects
    }

    fn cancel_deferred_submissions(&mut self, failure: FailureCode) {
        let deferred = std::mem::take(&mut self.deferred_submissions);
        if deferred.is_empty() {
            return;
        }
        let source_count = deferred
            .values()
            .map(|intent| intent.pending.sources.len())
            .sum::<usize>();
        for intent in deferred.into_values() {
            self.release_submission_sources(pending_source_ids(&intent.pending));
        }
        let kept = kept_sentence(source_count);
        let recovery = match failure {
            FailureCode::RecoveryCapacity => "press w to export recovery",
            _ => "press r to retry or w to export recovery",
        };
        self.set_error(format!(
            "Submission not started because changes were not saved. {kept}; {recovery}"
        ));
    }
}

pub(super) fn submission_progress(intent: &DeferredSubmissionIntent) -> &'static str {
    match (intent.pending.disposition, intent.pending.sources.len() > 1) {
        (SubmissionDisposition::Keep, false) => "submitting now, thought will be kept",
        (SubmissionDisposition::Keep, true) => "submitting now, thoughts will be kept",
        (SubmissionDisposition::RemoveAfterSuccess, false) => {
            "submitting now, thought will be removed after acceptance"
        }
        (SubmissionDisposition::RemoveAfterSuccess, true) => {
            "submitting now, thoughts will be removed after acceptance"
        }
    }
}
