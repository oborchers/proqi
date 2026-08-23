//! Verified directional submission and non-destructive completion.

use crate::{
    application::{Action, Effect},
    domain::{BoardOperationKind, Direction, SubmissionId},
    ports::{
        agent::{AgentError, AgentTarget, SubmissionReceipt, SubmissionRequest},
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
        self.status = Some("checking adjacent agents".to_owned());
        Self::discover_agents()
    }

    /// Replace targets only after complete verified discovery.
    pub fn complete_agent_discovery(&mut self, result: Result<Vec<AgentTarget>, AgentError>) {
        self.submission_mode = None;
        let was_refreshing = self.status.as_deref() == Some("checking adjacent agents");
        match result {
            Ok(targets) => {
                if was_refreshing {
                    self.status = Some(discovery_status(targets.len()));
                }
                self.agent_targets = targets;
            }
            Err(error @ (AgentError::Unavailable(_) | AgentError::Unsupported(_))) => {
                self.agent_targets.clear();
                if was_refreshing {
                    self.status = Some(format!("direct submission unavailable: {error}"));
                }
            }
            Err(error) => {
                self.agent_targets.clear();
                self.status = Some(format!("direct submission unavailable: {error}"));
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
                self.status = Some("submission receipt did not match the request".to_owned());
                return Vec::new();
            }
            Err(error) => {
                self.status = Some(format!("thought was not submitted: {error}"));
                return Vec::new();
            }
        };
        let mut effects = vec![Effect::StoreIntegrationContext {
            session_id: self.state.board.session.id,
            target: receipt.target.clone(),
            verified_at: pending.at,
        }];
        if pending.remove {
            effects.extend(self.reduce(Action::DeleteThought {
                operation_id: pending.operation_id,
                thought_id: pending.thought_id,
                kind: BoardOperationKind::SubmitAndRemove,
                at: pending.at,
            }));
        }
        self.status = Some(format!(
            "sent {} to {}",
            direction_name(receipt.target.direction),
            receipt.target.agent_name
        ));
        effects
    }

    /// Compact verified target description shown before a submission is committed.
    #[must_use]
    pub fn agent_hint(&self) -> Option<String> {
        if let Some(mode) = self.submission_mode {
            return Some(if mode.remove {
                "submit and remove: choose direction".to_owned()
            } else {
                "submit: choose direction".to_owned()
            });
        }
        match self.agent_targets.as_slice() {
            [target] => Some(format!(
                "send {} {} ({})",
                direction_name(target.direction),
                target.agent_name,
                readiness_name(target.readiness)
            )),
            targets if !targets.is_empty() => Some(format!("{} verified agents", targets.len())),
            _ => None,
        }
    }

    pub(super) fn begin_submission(
        &mut self,
        remove: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match self.agent_targets.as_slice() {
            [] => self.refresh_agents(),
            [target] => self.submit_to(target.direction, remove, ids, clock),
            _ => {
                self.submission_mode = Some(SubmissionMode { remove });
                self.status = Some("choose agent direction with arrows or h/j/k/l".to_owned());
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
        let remove = self.submission_mode?.remove;
        let direction = match input {
            UiInput::Key(UiKey::Escape) => {
                self.submission_mode = None;
                self.status = Some("submission cancelled".to_owned());
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
            UiInput::Resize { .. } => return None,
            _ => return Some(Vec::new()),
        };
        self.submission_mode = None;
        Some(self.submit_to(direction, remove, ids, clock))
    }

    pub(super) fn submit_to(
        &mut self,
        direction: Direction,
        remove: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(target) = self
            .agent_targets
            .iter()
            .find(|target| target.direction == direction)
            .cloned()
        else {
            self.status = Some(format!("no verified agent {}", direction_name(direction)));
            return Vec::new();
        };
        let Some(thought) = self
            .state
            .focused_thought
            .and_then(|id| self.state.board.thought(id))
            .filter(|thought| thought.is_live())
        else {
            self.status = Some("select a thought before submitting".to_owned());
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
                remove,
            },
        );
        self.status = Some("submitting thought".to_owned());
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

fn readiness_name(readiness: crate::ports::agent::AgentReadiness) -> &'static str {
    match readiness {
        crate::ports::agent::AgentReadiness::Idle => "idle",
        crate::ports::agent::AgentReadiness::Working => "working",
        crate::ports::agent::AgentReadiness::Done => "done",
    }
}

fn discovery_status(target_count: usize) -> String {
    match target_count {
        0 => "no verified adjacent agent".to_owned(),
        1 => "verified 1 adjacent agent".to_owned(),
        count => format!("verified {count} adjacent agents"),
    }
}
