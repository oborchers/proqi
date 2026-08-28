//! Durable duplication of one thought or an ordered board selection.

use crate::{
    application::{Action, Effect},
    ports::environment::{Clock, IdGenerator},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn duplicate(
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
        let duplicate_ids = thought_ids
            .iter()
            .map(|_| ids.thought_id())
            .collect::<Vec<_>>();
        let effects = self.reduce(Action::DuplicateThoughts {
            operation_id: ids.operation_id(),
            thought_ids,
            duplicate_ids: duplicate_ids.clone(),
            at: clock.now(),
        });
        if effects.is_empty() {
            return effects;
        }
        self.replace_board_selection(duplicate_ids.iter().copied());
        self.manual_board_scroll = false;
        effects
    }
}
