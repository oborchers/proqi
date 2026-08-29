//! Durable thought presentation commands.

use crate::{
    application::{Action, Effect},
    domain::ThoughtPresentation,
    ports::environment::{Clock, IdGenerator},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn expand_thought(
        &mut self,
        thought_id: crate::domain::ThoughtId,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.submission_locked(thought_id) {
            self.set_warning("thought has a submission in progress");
            return Vec::new();
        }
        if !self.activation_needs_expansion(thought_id) {
            return Vec::new();
        }
        self.reduce(Action::SetPresentation {
            operation_id: ids.operation_id(),
            thought_id,
            presentation: ThoughtPresentation::Expanded,
            at: clock.now(),
        })
    }

    pub(super) fn activation_needs_expansion(&self, thought_id: crate::domain::ThoughtId) -> bool {
        let Some(thought) = self.state.board.thought(thought_id) else {
            return false;
        };
        thought.presentation == ThoughtPresentation::Collapsed
            || thought.presentation == ThoughtPresentation::Automatic
                && self
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.thought(thought_id))
                    .is_some_and(|layout| layout.hidden_rows > 0)
    }

    pub(super) fn collapse(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let thought_ids = self.action_thought_ids();
        if thought_ids.is_empty() {
            return Vec::new();
        }
        if thought_ids.iter().any(|id| self.submission_locked(*id)) {
            self.set_warning("selected thought has a submission in progress");
            return Vec::new();
        }
        if thought_ids.len() > 1 {
            return self.collapse_selection(thought_ids, ids, clock);
        }
        self.collapse_one(thought_ids[0], ids, clock)
    }

    fn collapse_selection(
        &mut self,
        thought_ids: Vec<crate::domain::ThoughtId>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let presentation = if thought_ids.iter().any(|id| self.needs_expansion(*id)) {
            ThoughtPresentation::Expanded
        } else {
            ThoughtPresentation::Collapsed
        };
        self.reduce(Action::SetPresentationMany {
            operation_id: ids.operation_id(),
            thought_ids,
            presentation,
            at: clock.now(),
        })
    }

    fn collapse_one(
        &mut self,
        thought_id: crate::domain::ThoughtId,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought) = self.state.board.thought(thought_id) else {
            return Vec::new();
        };
        let presentation = thought.presentation;
        let capped = self
            .layout
            .as_ref()
            .and_then(|layout| layout.thought(thought_id))
            .is_some_and(|layout| layout.hidden_rows > 0);
        let next = match presentation {
            ThoughtPresentation::Automatic if capped => ThoughtPresentation::Expanded,
            ThoughtPresentation::Automatic | ThoughtPresentation::Expanded => {
                ThoughtPresentation::Collapsed
            }
            ThoughtPresentation::Collapsed => ThoughtPresentation::Expanded,
        };
        self.reduce(Action::SetPresentation {
            operation_id: ids.operation_id(),
            thought_id,
            presentation: next,
            at: clock.now(),
        })
    }

    fn needs_expansion(&self, thought_id: crate::domain::ThoughtId) -> bool {
        let mode = self
            .state
            .board
            .thought(thought_id)
            .map(|thought| thought.presentation);
        let hidden = self
            .layout
            .as_ref()
            .and_then(|layout| layout.thought(thought_id))
            .map(|layout| layout.hidden_rows > 0);
        matches!(mode, Some(ThoughtPresentation::Collapsed))
            || matches!(mode, Some(ThoughtPresentation::Automatic)) && hidden.unwrap_or(true)
    }
}
