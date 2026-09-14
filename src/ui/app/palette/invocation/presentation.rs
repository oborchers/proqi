//! Captured presentation scope and binding owner for Commands rows.

use crate::{
    application::InteractionMode,
    ui::{CommandMetadata, CommandScope, ShortcutActionId, ShortcutContext},
};

use super::CommandContext;

impl CommandContext {
    pub(in crate::ui::app::palette) fn command_binding_context(
        &self,
        action: ShortcutActionId,
        metadata: CommandMetadata,
    ) -> Option<ShortcutContext> {
        if metadata.scope == CommandScope::Commands {
            return Some(ShortcutContext::Commands);
        }
        if self.recovery.failed() {
            return Some(ShortcutContext::Recovery);
        }
        if metadata.shortcut_owner == ShortcutActionId::ContextualTransform {
            return (self.contextual_transform_action() == Some(action))
                .then_some(ShortcutContext::Board);
        }
        Some(match metadata.scope {
            CommandScope::Contextual => {
                ShortcutContext::surface(self.board.mode, self.board.insertion_focused, false)
            }
            CommandScope::Editor => ShortcutContext::Edit,
            CommandScope::Board
            | CommandScope::Selection
            | CommandScope::Session
            | CommandScope::Application => ShortcutContext::Board,
            CommandScope::Commands => ShortcutContext::Commands,
        })
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

    fn contextual_transform_action(&self) -> Option<ShortcutActionId> {
        if self.merge_applicability().enabled {
            return Some(ShortcutActionId::MergeThoughts);
        }
        let handoff = self.selection.editor_handoff.as_ref()?;
        Some(if handoff.has_selection() {
            ShortcutActionId::ExtractSelection
        } else {
            ShortcutActionId::SplitThought
        })
    }
}
