//! Validity checks for asynchronous clipboard-read ownership.

use crate::application::InteractionMode;

use super::super::{BoardApp, EditorOwner, pending_types::ClipboardReadOwner};

impl BoardApp {
    pub(super) fn clipboard_read_owner_is_current(&self, owner: ClipboardReadOwner) -> bool {
        match owner {
            ClipboardReadOwner::Board => matches!(self.state.mode, InteractionMode::Board),
            ClipboardReadOwner::Compose { generation } => {
                generation == self.compose_generation
                    && matches!(self.state.mode, InteractionMode::Compose)
                    && matches!(self.editor.as_ref(), Some((EditorOwner::Compose, _)))
            }
            ClipboardReadOwner::Thought {
                thought_id: expected,
                generation,
            } => {
                matches!(
                    self.state.mode,
                    InteractionMode::Edit { thought_id } if thought_id == expected
                ) && generation == self.edit_owner_generation
                    && matches!(
                        self.editor.as_ref(),
                        Some((EditorOwner::Thought(actual), _)) if *actual == expected
                    )
                    && !self.edit_content_mutation_blocked(expected)
            }
        }
    }
}
