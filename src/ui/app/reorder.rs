//! Single-thought keyboard and pointer reorder behavior.

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
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        if self.submission_locked(thought_id) {
            self.set_warning("thought has a submission in progress");
            return Vec::new();
        }
        if self.selection_len() > 1 {
            self.set_warning("reordering is unavailable for multiple selected thoughts");
            return Vec::new();
        }
        let live = self.state.board.live_thoughts();
        let Some(current) = live.iter().position(|thought| thought.id == thought_id) else {
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
        self.reduce(Action::MoveThought {
            operation_id: ids.operation_id(),
            thought_id,
            to: target,
            at: clock.now(),
        })
    }
}
