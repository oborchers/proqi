//! Source capture and directional choice for verified adjacent-agent delivery.

use crate::{
    application::{Effect, InteractionMode},
    domain::{Direction, ThoughtId},
    ports::{
        agent::SubmissionDisposition,
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, UiInput, UiKey, pending_types::SubmissionMode};

impl BoardApp {
    pub(super) fn begin_edit_delivery(
        &mut self,
        disposition: SubmissionDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.active_thought_id() else {
            return Vec::new();
        };
        let mut effects = self.flush_pending_edit(ids, clock);
        effects.extend(self.begin_delivery_for(disposition, vec![thought_id], ids, clock));
        effects
    }

    pub(super) fn begin_delivery(
        &mut self,
        disposition: SubmissionDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.deactivate_range_latch();
        if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            return self.begin_edit_delivery(disposition, ids, clock);
        }
        let mut effects = self.flush_pending_edit(ids, clock);
        effects.extend(self.begin_delivery_for(disposition, self.action_thought_ids(), ids, clock));
        effects
    }

    pub(super) fn begin_delivery_all(
        &mut self,
        disposition: SubmissionDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let mut effects = if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            self.finish_edit(ids, clock)
        } else {
            self.flush_pending_edit(ids, clock)
        };
        let thought_ids = self
            .state
            .board
            .live_thoughts()
            .into_iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>();
        if thought_ids.is_empty() {
            self.set_info("board is empty; nothing submitted");
            return effects;
        }
        effects.extend(self.begin_delivery_for(disposition, thought_ids, ids, clock));
        effects
    }

    fn begin_delivery_for(
        &mut self,
        disposition: SubmissionDisposition,
        thought_ids: Vec<ThoughtId>,
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
            [direction] => self.deliver_thoughts(*direction, disposition, &thought_ids, ids, clock),
            _ => {
                self.submission_mode = Some(SubmissionMode {
                    disposition,
                    thought_ids,
                });
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
        let disposition = self.submission_mode.as_ref()?.disposition;
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
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Pointer(_) => return None,
            _ => return Some(Vec::new()),
        };
        Some(self.deliver_to(direction, disposition, ids, clock))
    }

    pub(super) fn deliver_to(
        &mut self,
        direction: Direction,
        disposition: SubmissionDisposition,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let thought_ids = self
            .submission_mode
            .take()
            .filter(|mode| mode.disposition == disposition)
            .map_or_else(|| self.action_thought_ids(), |mode| mode.thought_ids);
        self.deliver_thoughts(direction, disposition, &thought_ids, ids, clock)
    }

    fn deliver_thoughts(
        &mut self,
        direction: Direction,
        disposition: SubmissionDisposition,
        thought_ids: &[ThoughtId],
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(target) = self
            .agent_targets
            .iter()
            .find(|target| target.direction == direction)
            .cloned()
        else {
            self.set_warning(format!("no verified agent {}", direction.as_str()));
            return Vec::new();
        };
        if !target.delivery.supports() {
            self.set_warning(format!("submission is unavailable {}", direction.as_str()));
            return Vec::new();
        }
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
        self.queue_submission(&target, disposition, thought_ids, ids, clock)
    }
}
