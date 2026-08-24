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
    },
};

use super::{BoardApp, PendingSubmission, SubmissionMode, UiInput, UiKey};

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
            Err(error) => {
                self.agent_targets.clear();
                self.set_error(format!("direct submission unavailable: {error}"));
            }
        }
    }

    /// Complete one semantic prompt request. Failure never mutates the thought.
    pub fn complete_submission(
        &mut self,
        submission_id: SubmissionId,
        result: Result<SubmissionReceipt, AgentError>,
    ) -> Vec<Effect> {
        let Some(pending) = self.pending_submissions.remove(&submission_id) else {
            return Vec::new();
        };
        let receipt = match result {
            Ok(receipt)
                if receipt.submission_id == submission_id
                    && receipt.target == pending.request.target =>
            {
                receipt
            }
            Ok(_) => {
                self.set_error("Submission failed. Thought kept. Receipt did not match request.");
                return Vec::new();
            }
            Err(error) => {
                self.set_error(format!("Submission failed. Thought kept. {error}"));
                return Vec::new();
            }
        };
        let mut effects = vec![Effect::StoreIntegrationContext {
            session_id: self.state.board.session.id,
            target: receipt.target.clone(),
            verified_at: pending.at,
        }];
        if pending.disposition == SubmissionDisposition::RemoveAfterSuccess {
            effects.extend(self.reduce(Action::DeleteThought {
                operation_id: pending.operation_id,
                thought_id: pending.thought_id,
                kind: BoardOperationKind::SubmitAndRemove,
                at: pending.at,
            }));
        }
        let outcome = match pending.disposition {
            SubmissionDisposition::Keep => "thought kept",
            SubmissionDisposition::RemoveAfterSuccess => "thought removed",
        };
        self.set_success(format!(
            "submitted {} to {}, {outcome}",
            direction_name(receipt.target.direction),
            receipt.target.agent_name
        ));
        effects
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
        let Some(thought) = self
            .state
            .focused_thought
            .and_then(|id| self.state.board.thought(id))
            .filter(|thought| thought.is_live())
        else {
            self.set_warning("select a thought before submitting");
            return Vec::new();
        };
        let submission_id = ids.submission_id();
        let request = SubmissionRequest {
            submission_id,
            target,
            content: thought.content.clone(),
        };
        self.pending_submissions.insert(
            submission_id,
            PendingSubmission {
                request: request.clone(),
                thought_id: thought.id,
                operation_id: ids.operation_id(),
                at: clock.now(),
                disposition,
            },
        );
        self.set_info(match disposition {
            SubmissionDisposition::Keep => "submitting, thought will be kept",
            SubmissionDisposition::RemoveAfterSuccess => {
                "submitting, thought will be removed after acceptance"
            }
        });
        vec![Effect::SubmitAgent(request)]
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Right => "right",
        Direction::Down => "down",
        Direction::Left => "left",
    }
}

fn discovery_status(target_count: usize) -> String {
    match target_count {
        0 => "no verified adjacent agent".to_owned(),
        1 => "verified 1 adjacent agent".to_owned(),
        count => format!("verified {count} adjacent agents"),
    }
}
