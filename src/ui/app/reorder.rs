//! Single-item keyboard and pointer reorder behavior.

use crate::{
    application::{Action, Effect},
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
        if item_id
            .thought()
            .is_some_and(|thought_id| self.submission_locked(thought_id))
        {
            self.set_warning("thought has a submission in progress");
            return Vec::new();
        }
        if self.selection_len() > 1 {
            self.set_warning("reordering is unavailable for multiple selected items");
            return Vec::new();
        }
        let live = self.state.board.live_items();
        let Some(current) = live.iter().position(|item| item.id() == item_id) else {
            return Vec::new();
        };
        if live.len() <= 1 {
            return Vec::new();
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
}
