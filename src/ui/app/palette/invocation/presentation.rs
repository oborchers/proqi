//! Captured presentation scope and binding owner for Commands rows.

use crate::{
    application::InteractionMode,
    ui::{CommandScope, ShortcutContext},
};

use super::CommandContext;

impl CommandContext {
    pub(in crate::ui::app::palette) fn command_binding_context(
        &self,
        scope: CommandScope,
    ) -> ShortcutContext {
        if self.recovery.failed() {
            return ShortcutContext::Recovery;
        }
        match scope {
            CommandScope::Contextual => {
                ShortcutContext::surface(self.board.mode, self.board.insertion_focused, false)
            }
            CommandScope::Editor => ShortcutContext::Edit,
            CommandScope::Board
            | CommandScope::Selection
            | CommandScope::Session
            | CommandScope::Application => ShortcutContext::Board,
        }
    }

    pub(in crate::ui::app::palette) const fn command_scope_label(
        &self,
        scope: CommandScope,
    ) -> &'static str {
        match scope {
            CommandScope::Contextual => match self.board.mode {
                InteractionMode::Board => "board",
                InteractionMode::Compose => "compose",
                InteractionMode::Edit { .. } => "edit",
            },
            _ => scope.label(),
        }
    }
}
