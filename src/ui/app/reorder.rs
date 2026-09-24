//! Keyboard reorder behavior for one item or selected Board runs.

use crate::{
    application::{Action, Effect, selected_move_steps},
    domain::ThoughtId,
    ports::environment::{Clock, IdGenerator},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn reorder(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        delta: isize,
    ) -> Vec<Effect> {
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        let Some(item_id) = self.state.focused_item else {
            return Vec::new();
        };
        if self.selection_len() <= 1
            && item_id
                .thought()
                .is_some_and(|thought_id| !self.thought_mutable(thought_id))
        {
            self.reorder_lock_warning(&item_id.thought().into_iter().collect::<Vec<_>>());
            return Vec::new();
        }
        let live = self.state.board.live_items();
        let Some(current) = live.iter().position(|item| item.id() == item_id) else {
            return Vec::new();
        };
        if live.len() <= 1 {
            return Vec::new();
        }
        if self.selection_len() > 1 {
            let selected = self.action_item_ids();
            let order = live.iter().map(|item| item.id()).collect::<Vec<_>>();
            let Ok(steps) = selected_move_steps(&order, &selected, delta) else {
                return Vec::new();
            };
            if steps.is_empty() {
                self.set_info("selected items are already at the edge");
                return Vec::new();
            }
            let touched = selected
                .iter()
                .chain(steps.iter().map(|step| &step.item_id))
                .filter_map(|id| id.thought())
                .collect::<Vec<_>>();
            if touched.iter().any(|id| !self.thought_mutable(*id)) {
                self.reorder_lock_warning(&touched);
                return Vec::new();
            }
            let effects = self.reduce(Action::MoveItems {
                operation_id: ids.operation_id(),
                item_ids: selected.clone(),
                delta,
                at: clock.now(),
            });
            if effects
                .iter()
                .any(|effect| matches!(effect, Effect::CommitBoardOperation(_)))
            {
                self.replace_board_selection(selected);
            }
            return effects;
        }
        let target = if delta < 0 {
            current.checked_sub(1).unwrap_or(live.len() - 1)
        } else if current + 1 == live.len() {
            0
        } else {
            current + 1
        };
        self.reduce(Action::MoveItem {
            operation_id: ids.operation_id(),
            item_id,
            to: target,
            at: clock.now(),
        })
    }

    fn reorder_lock_warning(&mut self, ids: &[ThoughtId]) {
        if ids.iter().any(|id| self.state.thought_locked(*id)) {
            self.set_warning("thought has a submission in progress");
        } else {
            self.set_warning("thought has an operation in progress");
        }
    }
}
