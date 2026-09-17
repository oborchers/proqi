//! Durable duplication of one item or an ordered board selection.

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
        let item_ids = self.action_item_ids();
        if item_ids.is_empty() {
            return Vec::new();
        }
        if item_ids
            .iter()
            .filter_map(|id| id.thought())
            .any(|id| self.submission_locked(id))
        {
            self.set_warning("selected thought has a submission in progress");
            return Vec::new();
        }
        let duplicate_ids = item_ids
            .iter()
            .map(|item| match item {
                crate::domain::BoardItemId::Thought(_) => ids.thought_id().into(),
                crate::domain::BoardItemId::Separator(_) => ids.separator_id().into(),
            })
            .collect::<Vec<_>>();
        let effects = self.reduce(Action::DuplicateItems {
            operation_id: ids.operation_id(),
            item_ids,
            duplicate_ids: duplicate_ids.clone(),
            at: clock.now(),
        });
        if effects.is_empty() {
            return effects;
        }
        self.replace_board_selection(duplicate_ids.iter().copied());
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        effects
    }
}
