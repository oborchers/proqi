//! Single-thought keyboard and pointer reorder behavior.

use crate::{
    application::{Action, Effect},
    ports::{
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
    },
};

use super::BoardApp;
use crate::ui::settings::BoardCommand;

impl BoardApp {
    pub(super) fn reorder_from_character(
        &mut self,
        character: char,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match self.settings.keybindings.command(character) {
            Some(BoardCommand::RangeUp) => self.reorder(ids, clock, -1),
            Some(BoardCommand::RangeDown) => self.reorder(ids, clock, 1),
            _ => Vec::new(),
        }
    }

    pub(super) fn reorder_from_movement(
        &mut self,
        movement: CursorMovement,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match movement {
            CursorMovement::VisualUp | CursorMovement::DocumentStart => {
                self.reorder(ids, clock, -1)
            }
            CursorMovement::VisualDown | CursorMovement::DocumentEnd => self.reorder(ids, clock, 1),
            CursorMovement::GraphemeBack
            | CursorMovement::GraphemeForward
            | CursorMovement::WordBack
            | CursorMovement::WordForward
            | CursorMovement::LineStart
            | CursorMovement::LineEnd => Vec::new(),
        }
    }

    pub(super) fn reorder(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        delta: isize,
    ) -> Vec<Effect> {
        self.manual_board_scroll = false;
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
