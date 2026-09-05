//! One-shot editor selection transfer through the command palette.

use crate::{
    application::InteractionMode,
    domain::{ContentAnnotation, TextPosition, ThoughtId},
    ports::editor::{EditCommand, TextSelection},
    ui::{HitTarget, PointerButton, PointerKind, UiInput},
};

use super::BoardApp;

#[derive(Clone)]
pub(super) struct EditorSelectionHandoff {
    pub(super) thought_id: ThoughtId,
    pub(super) content: String,
    pub(super) annotations: Vec<ContentAnnotation>,
    pub(super) selection: Option<TextSelection>,
    pub(super) cursor: TextPosition,
}

impl EditorSelectionHandoff {
    pub(super) const fn has_selection(&self) -> bool {
        self.selection.is_some()
    }
}

impl BoardApp {
    pub(super) fn palette_handoff_is_current(&self, handoff: &EditorSelectionHandoff) -> bool {
        self.state.focused_thought == Some(handoff.thought_id)
            && self
                .state
                .board
                .thought(handoff.thought_id)
                .is_some_and(|thought| {
                    thought.is_live()
                        && thought.content == handoff.content
                        && thought.annotations == handoff.annotations
                })
    }

    pub(super) fn capture_palette_selection_handoff(&mut self) {
        self.palette_selection_handoff = self.editor_snapshot().and_then(|snapshot| {
            let thought_id = self.active_thought_id()?;
            let annotations = self.current_annotations(thought_id);
            Some(EditorSelectionHandoff {
                thought_id,
                content: snapshot.content,
                annotations,
                selection: snapshot.selection,
                cursor: snapshot.cursor,
            })
        });
    }

    pub(super) fn invalidate_palette_selection_handoff(&mut self, input: &UiInput) {
        let preserves = match input {
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::HostFocusLost => true,
            UiInput::Pointer(pointer) if matches!(pointer.kind, PointerKind::Move) => true,
            UiInput::Key(key) => {
                matches!(
                    self.shortcut_registry.board_action_for_intention(*key),
                    Some(
                        crate::ui::ShortcutActionId::OpenCommands
                            | crate::ui::ShortcutActionId::ContextualTransform
                    )
                )
            }
            UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) =>
            {
                self.hit(*pointer) == Some(HitTarget::Commands)
            }
            UiInput::KeyStroke(_)
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
