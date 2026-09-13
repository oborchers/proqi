//! Contextual durable history commands.

use crate::{
    application::{Action, Effect, HistoryResolution},
    ports::environment::{Clock, IdGenerator},
};

use super::super::{BoardApp, pending_types::EditFlush};

impl BoardApp {
    pub(in crate::ui::app) fn history(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        undo: bool,
    ) -> Vec<Effect> {
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let scope = match self.state.history_resolution(self.state.mode, undo) {
            HistoryResolution::Ready(scope) => scope,
            HistoryResolution::Empty => {
                self.set_info(if undo {
                    "Nothing to undo"
                } else {
                    "Nothing to redo"
                });
                return effects;
            }
            HistoryResolution::BlockedByEditor { .. } => {
                self.set_info(if undo {
                    "Undo unavailable: undo the newer edit in the affected thought first"
                } else {
                    "Redo unavailable: redo the earlier edit in the affected thought first"
                });
                return effects;
            }
            HistoryResolution::BlockedByBoard { .. } => {
                self.set_info(if undo {
                    "Undo unavailable here: exit edit to undo newer Board changes first"
                } else {
                    "Redo unavailable here: exit edit to redo earlier Board changes first"
                });
                return effects;
            }
        };
        let action = if undo {
            Action::Undo {
                operation_id: ids.operation_id(),
                scope,
                at: clock.now(),
            }
        } else {
            Action::Redo {
                operation_id: ids.operation_id(),
                scope,
                at: clock.now(),
            }
        };
        effects.extend(self.reduce_with_empty_transition(
            action,
            crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
        ));
        self.reload_editor();
        self.sync_empty_insertion_focus();
        effects
    }
}
