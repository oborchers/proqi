//! Configured command bindings use the Commands applicability and execution owner.
use super::{BoardApp, invocation::CommandContext};
use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
    ui::CommandApplicability,
};

impl BoardApp {
    pub(in crate::ui::app) fn execute_bound_command(
        &mut self,
        action: crate::ui::ShortcutActionId,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let unavailable_board_thought = self
            .settings
            .shortcuts
            .descriptor(action)
            .and_then(|descriptor| descriptor.commands)
            .is_some_and(|metadata| {
                metadata.applicability == CommandApplicability::BoardThought
                    && !self.board_thought_command_available()
            });
        if unavailable_board_thought {
            self.set_warning("command is unavailable in the current state");
            return Vec::new();
        }
        if self.editor_snapshot().is_some() {
            self.capture_palette_selection_handoff();
        }
        let captured = (self.palette.is_none()
            && !matches!(
                action,
                crate::ui::ShortcutActionId::OpenCommands | crate::ui::ShortcutActionId::OpenSearch
            ))
        .then(|| self.capture_command_context());
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            crate::ui::app::pending_types::EditFlush::Complete(effects) => effects,
            crate::ui::app::pending_types::EditFlush::Blocked(effects) => return effects,
        };
        effects.extend(self.execute_flushed_bound_command(action, captured, ids, clock));
        effects
    }

    fn execute_flushed_bound_command(
        &mut self,
        action: crate::ui::ShortcutActionId,
        captured: Option<CommandContext>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        use crate::ui::ShortcutActionId as Shortcut;
        match action {
            Shortcut::OpenCommands => {
                if self.editor_snapshot().is_some() {
                    self.capture_palette_selection_handoff();
                }
                self.open_palette();
                return Vec::new();
            }
            Shortcut::OpenSearch => {
                self.open_search();
                return Vec::new();
            }
            _ => {}
        }
        let command = self
            .settings
            .shortcuts
            .descriptor(action)
            .and_then(|descriptor| descriptor.commands.zip(descriptor.command_execution));
        let Some((metadata, execution)) = command else {
            return Vec::new();
        };
        let mut context = self
            .palette
            .take()
            .map(|palette| palette.context)
            .or(captured)
            .unwrap_or_else(|| self.capture_command_context());
        self.palette_selection_handoff = None;
        let applicability = context.applicability(metadata);
        if !applicability.enabled {
            self.set_warning(
                applicability
                    .reason
                    .unwrap_or("command is unavailable in the current state"),
            );
            return Vec::new();
        }
        let selection_handoff = context.take_selection_handoff();
        let merge_handoff = context.take_merge_handoff();
        self.execute_command(
            execution,
            selection_handoff,
            merge_handoff.as_deref(),
            ids,
            clock,
        )
    }
}
