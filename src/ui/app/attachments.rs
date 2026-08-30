//! UI composition for application-owned attachment scheduling and preflight outcomes.

use std::time::Duration;

use crate::{
    application::{
        AttachmentPreflightOutcome, AttachmentRefreshCause, AttachmentRefreshOutcome, Effect,
        attachment_keys,
    },
    domain::ThoughtId,
    ports::attachment_accessibility::AttachmentCheckBatchResult,
};

use super::{BoardApp, agent::pending_source_ids, pending_types::DeferredSubmissionIntent};

impl BoardApp {
    pub(super) fn may_change_attachments(action: &crate::application::Action) -> bool {
        matches!(
            action,
            crate::application::Action::CreateThought { .. }
                | crate::application::Action::PasteAsThought { .. }
                | crate::application::Action::EditThought { .. }
                | crate::application::Action::DeleteThought { .. }
                | crate::application::Action::DeleteThoughts { .. }
                | crate::application::Action::DuplicateThoughts { .. }
                | crate::application::Action::Undo { .. }
                | crate::application::Action::Redo { .. }
        )
    }

    pub(super) fn finish_attachment_mutation(&mut self, may_change: bool) {
        if !may_change {
            return;
        }
        if self.state.attachments.manual_refresh_active() {
            self.set_attachment_info("refreshing attachments");
        } else {
            self.clear_attachment_status();
        }
    }

    /// Start focused-first transient checks without delaying restored board use.
    pub fn start_attachment_checks(&mut self, now: Duration) -> Vec<Effect> {
        self.state
            .attachments
            .start(&self.state.board, self.state.focused_thought, now)
    }

    /// Refresh all current keys for a manual command or debounced host-focus event.
    pub fn refresh_attachments(&mut self, manual: bool) -> Vec<Effect> {
        if !manual && self.state.attachments.manual_refresh_active() {
            return Vec::new();
        }
        if manual {
            self.set_attachment_info("refreshing attachments");
        } else {
            self.clear_attachment_status();
        }
        let cause = if manual {
            AttachmentRefreshCause::Manual
        } else {
            AttachmentRefreshCause::Quiet
        };
        let (effects, outcome) = self.state.attachments.refresh_all(
            &self.state.board,
            self.state.focused_thought,
            cause,
        );
        if let Some(outcome) = outcome {
            self.finish_attachment_refresh(outcome);
        }
        effects
    }

    /// Trigger the documented fallback once after a bounded inactive interval.
    pub fn note_attachment_interaction(&mut self, now: Duration) -> Vec<Effect> {
        let (effects, refreshed) = self.state.attachments.note_deliberate_interaction(
            &self.state.board,
            self.state.focused_thought,
            now,
        );
        if refreshed {
            self.clear_attachment_status();
        }
        effects
    }

    /// Apply one bounded worker result and continue background or preflight work.
    pub fn complete_attachment_checks(
        &mut self,
        completion: AttachmentCheckBatchResult,
    ) -> Vec<Effect> {
        let (mut effects, preflight, refresh) = self.state.attachments.complete(completion);
        if let Some(outcome) = preflight {
            effects.extend(self.complete_attachment_preflight(outcome));
        }
        if let Some(outcome) = refresh {
            self.finish_attachment_refresh(outcome);
        }
        effects
    }

    /// Binary render state for one current annotation.
    #[must_use]
    pub(in crate::ui) fn attachment_inaccessible(
        &self,
        thought_id: ThoughtId,
        annotation_index: usize,
    ) -> bool {
        self.state
            .attachments
            .inaccessible(thought_id, annotation_index)
    }

    pub(super) fn begin_submission_preflight(
        &mut self,
        intent: DeferredSubmissionIntent,
    ) -> Vec<Effect> {
        let submission_id = intent.attempt.id;
        let keys = intent.attachment_keys.clone();
        self.preflight_submissions.insert(submission_id, intent);
        self.set_info("checking attachments");
        let (mut effects, outcome) = self.state.attachments.begin_preflight(submission_id, keys);
        if let Some(outcome) = outcome {
            effects.extend(self.complete_attachment_preflight(outcome));
        }
        effects
    }

    fn complete_attachment_preflight(
        &mut self,
        outcome: AttachmentPreflightOutcome,
    ) -> Vec<Effect> {
        let Some(mut intent) = self.preflight_submissions.remove(&outcome.submission_id) else {
            return Vec::new();
        };
        let source_ids = pending_source_ids(&intent.pending);
        let current_keys = source_ids
            .iter()
            .try_fold(Vec::new(), |mut keys, thought_id| {
                let thought = self
                    .state
                    .board
                    .thought(*thought_id)
                    .filter(|thought| thought.is_live())?;
                keys.extend(attachment_keys(thought));
                Some(keys)
            });
        let sources_unchanged = current_keys.as_ref() == Some(&intent.attachment_keys)
            && intent.pending.sources.iter().all(|source| {
                self.current_thought_digest(source.thought_id) == Some(source.source_digest)
            });
        if !sources_unchanged {
            self.release_submission_sources(source_ids);
            self.set_warning("board changed during attachment check; thoughts kept");
            return Vec::new();
        }
        if outcome.inaccessible > 0 {
            self.release_submission_sources(source_ids);
            self.set_error(inaccessible_message(outcome.inaccessible));
            return Vec::new();
        }
        intent.attempt.source_sequence = self.state.board.session.last_durable_sequence;
        let submission_id = intent.attempt.id;
        let progress = super::agent_preparation::submission_progress(&intent);
        self.pending_submissions
            .insert(submission_id, intent.pending);
        self.set_info(progress);
        vec![Effect::PrepareSubmission(intent.attempt)]
    }

    fn finish_attachment_refresh(&mut self, outcome: AttachmentRefreshOutcome) {
        if outcome.total == 0 {
            self.set_attachment_info("no attachments to refresh");
        } else if outcome.inaccessible == 0 {
            self.set_attachment_success("all attachments are accessible");
        } else {
            self.set_attachment_warning(inaccessible_message(outcome.inaccessible));
        }
    }
}

fn inaccessible_message(count: usize) -> String {
    if count == 1 {
        "Proqi cannot access 1 attachment".to_owned()
    } else {
        format!("Proqi cannot access {count} attachments")
    }
}
