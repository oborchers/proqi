//! Verified directional submission and non-destructive completion.

use crate::{
    application::{Action, Effect},
    domain::{BoardOperationKind, Direction, SubmissionId},
    ports::{
        agent::{
            AgentError, AgentTarget, SubmissionDisposition, SubmissionReceipt, SubmissionRequest,
        },
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
        store::{StoreError, SubmissionAttempt, SubmissionAttemptState, SubmissionOutcome},
    },
};
use sha2::{Digest, Sha256};

use super::{
    BoardApp, UiInput, UiKey,
    pending_types::{PendingSubmission, PendingSubmissionSource, SubmissionMode},
};

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
            self.pending_submissions.remove(&submission_id);
            self.set_error(format!("Submission not started. Thought kept. {error}"));
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
            self.pending_submissions.remove(&submission_id);
            self.set_error(format!("Submission not started. Thought kept. {error}"));
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
                    && receipt.target.identity() == pending.request.target.identity() =>
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
        let Some(completion) = pending.completion.take() else {
            return Vec::new();
        };
        if let Err(error) = result {
            let status = if completion.is_ok() {
                "accepted"
            } else {
                "failed"
            };
            self.set_error(format!(
                "Submission {status}, but its outcome was not saved. Thought kept. {error}"
            ));
            return Vec::new();
        }
        match completion {
            Ok(receipt) => self.apply_accepted_submission(&pending, &receipt),
            Err(error) => {
                self.set_error(format!("Submission failed. Thought kept. {error}"));
                Vec::new()
            }
        }
    }

    pub(super) fn begin_delivery(
        &mut self,
        disposition: SubmissionDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.agent_targets.is_empty() {
            return self.refresh_agents();
        }
        let eligible = self
            .agent_targets
            .iter()
            .filter(|target| target.delivery.supports())
            .map(|target| target.direction)
            .collect::<Vec<_>>();
        match eligible.as_slice() {
            [] => {
                self.set_warning("submission is unavailable for verified adjacent agents");
                Vec::new()
            }
            [direction] => self.deliver_to(*direction, disposition, ids, clock),
            _ => {
                self.submission_mode = Some(SubmissionMode { disposition });
                self.set_info("choose agent direction with arrows or h/j/k/l");
                Vec::new()
            }
        }
    }

    pub(super) fn handle_submission_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        let mode = self.submission_mode?;
        let direction = match input {
            UiInput::Key(UiKey::Escape) => {
                self.submission_mode = None;
                self.set_info("submission cancelled");
                return Some(Vec::new());
            }
            UiInput::Key(
                UiKey::Character('h')
                | UiKey::Move {
                    movement: CursorMovement::GraphemeBack,
                    ..
                },
            ) => Direction::Left,
            UiInput::Key(
                UiKey::Character('l')
                | UiKey::Move {
                    movement: CursorMovement::GraphemeForward,
                    ..
                },
            ) => Direction::Right,
            UiInput::Key(
                UiKey::Character('k')
                | UiKey::Move {
                    movement: CursorMovement::VisualUp,
                    ..
                },
            ) => Direction::Up,
            UiInput::Key(
                UiKey::Character('j')
                | UiKey::Move {
                    movement: CursorMovement::VisualDown,
                    ..
                },
            ) => Direction::Down,
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::Pointer(_) => return None,
            _ => return Some(Vec::new()),
        };
        self.submission_mode = None;
        Some(self.deliver_to(direction, mode.disposition, ids, clock))
    }

    pub(super) fn deliver_to(
        &mut self,
        direction: Direction,
        disposition: SubmissionDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(target) = self
            .agent_targets
            .iter()
            .find(|target| target.direction == direction)
            .cloned()
        else {
            self.set_warning(format!("no verified agent {}", direction_name(direction)));
            return Vec::new();
        };
        if !target.delivery.supports() {
            self.set_warning(format!(
                "submission is unavailable {}",
                direction_name(direction)
            ));
            return Vec::new();
        }
        let thought_ids = self.action_thought_ids();
        if thought_ids.is_empty() {
            self.set_warning("select a thought before submitting");
            return Vec::new();
        }
        if thought_ids.iter().any(|id| self.submission_locked(*id)) {
            let message = if thought_ids.len() == 1 {
                "this thought already has a submission in progress"
            } else {
                "a selected thought already has a submission in progress"
            };
            self.set_warning(message);
            return Vec::new();
        }
        self.queue_submission(&target, disposition, &thought_ids, ids, clock)
    }

    fn queue_submission(
        &mut self,
        target: &AgentTarget,
        disposition: SubmissionDisposition,
        thought_ids: &[crate::domain::ThoughtId],
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let source_contents = thought_ids
            .iter()
            .filter_map(|id| self.state.board.thought(*id))
            .map(|thought| (thought.id, thought.content.clone()))
            .collect::<Vec<_>>();
        let content = source_contents
            .iter()
            .map(|(_, content)| content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let submission_id = ids.submission_id();
        let payload_digest = digest(content.as_bytes());
        let at = clock.now();
        let request = SubmissionRequest {
            submission_id,
            target: target.clone(),
            content,
        };
        let sources = source_contents
            .iter()
            .map(|(thought_id, content)| PendingSubmissionSource {
                thought_id: *thought_id,
                source_digest: digest(content.as_bytes()),
            })
            .collect::<Vec<_>>();
        let deletion_operation_id = ids.operation_id();
        self.pending_submissions.insert(
            submission_id,
            PendingSubmission {
                request: request.clone(),
                sources,
                at,
                disposition,
                deletion_operation_id,
                completion: None,
            },
        );
        self.set_info(match disposition {
            SubmissionDisposition::Keep => "submitting now, thought will be kept",
            SubmissionDisposition::RemoveAfterSuccess => {
                "submitting now, thought will be removed after acceptance"
            }
        });
        let attempt = SubmissionAttempt {
            id: submission_id,
            session_id: self.state.board.session.id,
            sources: source_contents
                .into_iter()
                .map(
                    |(thought_id, content)| crate::ports::store::SubmissionSource {
                        thought_id,
                        source_digest: digest(content.as_bytes()),
                    },
                )
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
        vec![Effect::PrepareSubmission(attempt)]
    }

    fn apply_accepted_submission(
        &mut self,
        pending: &PendingSubmission,
        receipt: &SubmissionReceipt,
    ) -> Vec<Effect> {
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
            self.selected_thoughts.clear();
        }
        let outcome = if pending.disposition == SubmissionDisposition::Keep {
            "thought kept"
        } else if unchanged {
            "thought removed"
        } else {
            "thought changed during submission and was kept"
        };
        self.set_success(format!(
            "submitted {} to {}, {outcome}",
            direction_name(receipt.target.direction),
            receipt.target.agent_name
        ));
        effects
    }

    fn current_thought_digest(&self, thought_id: crate::domain::ThoughtId) -> Option<[u8; 32]> {
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

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn target_fingerprint(target: &AgentTarget) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let identity = target.identity();
    for field in [
        identity.provider.as_str(),
        identity.workspace_id.as_str(),
        identity.tab_id.as_str(),
        identity.source_pane_id.as_str(),
        identity.target_pane_id.as_str(),
        identity.direction.as_str(),
        identity.agent_kind.as_str(),
        identity.agent_session_id.as_str(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    hasher.finalize().into()
}

fn direction_name(direction: Direction) -> &'static str {
    direction.as_str()
}

fn discovery_status(target_count: usize) -> String {
    match target_count {
        0 => "no verified adjacent agent".to_owned(),
        1 => "verified 1 adjacent agent".to_owned(),
        count => format!("verified {count} adjacent agents"),
    }
}
