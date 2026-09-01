//! One-shot editor selection transfer through the command palette.

use crate::{
    application::InteractionMode,
    domain::{TextPosition, ThoughtId},
    ports::editor::{EditCommand, TextSelection},
    ui::{HitTarget, PointerButton, PointerKind, UiInput, UiKey},
};

use super::BoardApp;

pub(super) struct EditorSelectionHandoff {
    thought_id: ThoughtId,
    content: String,
    selection: Option<TextSelection>,
    cursor: TextPosition,
}

impl BoardApp {
    pub(super) fn capture_palette_selection_handoff(&mut self) {
        self.palette_selection_handoff = self.editor_snapshot().and_then(|snapshot| {
            Some(EditorSelectionHandoff {
                thought_id: self.active_thought_id()?,
                content: snapshot.content,
                selection: snapshot.selection,
                cursor: snapshot.cursor,
            })
        });
    }

    pub(super) fn invalidate_palette_selection_handoff(&mut self, input: &UiInput) {
        let preserves = match input {
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::HostFocusLost => true,
            UiInput::Pointer(pointer) if matches!(pointer.kind, PointerKind::Move) => true,
            UiInput::Key(UiKey::Character(character)) => {
                self.settings.keybindings.command(*character)
                    == Some(crate::ui::settings::BoardCommand::Commands)
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.settings.keybindings.command(' ')
                    == Some(crate::ui::settings::BoardCommand::Commands)
            }
            UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) =>
            {
                self.hit(*pointer) == Some(HitTarget::Commands)
            }
            UiInput::Key(_)
            | UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_) => false,
        };
        if !preserves {
            self.palette_selection_handoff = None;
        }
    }

    pub(super) fn restore_palette_selection_handoff(
        &mut self,
        handoff: Option<EditorSelectionHandoff>,
    ) {
        let Some(handoff) = handoff else {
            return;
        };
        let valid = matches!(
            self.state.mode,
            InteractionMode::Edit { thought_id } if thought_id == handoff.thought_id
        ) && self
            .state
            .board
            .thought(handoff.thought_id)
            .is_some_and(|thought| thought.content == handoff.content);
        if !valid {
            return;
        }
        let Some(selection) = handoff.selection else {
            self.apply_edit(EditCommand::SetCursor {
                position: handoff.cursor,
                extend_selection: false,
            });
            return;
        };
        let anchor = if handoff.cursor == selection.start {
            selection.end
        } else if handoff.cursor == selection.end {
            selection.start
        } else {
            return;
        };
        self.apply_edit(EditCommand::SetCursor {
            position: anchor,
            extend_selection: false,
        });
        self.apply_edit(EditCommand::SetCursor {
            position: handoff.cursor,
            extend_selection: true,
        });
    }
}
