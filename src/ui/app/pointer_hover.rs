//! Passive pointer emphasis derived from the current rendered frame.

use crate::ui::{HitTarget, PointerInput, PointerKind};

use super::{BoardApp, UiInput};

impl BoardApp {
    pub(super) fn track_hover_input(&mut self, input: &UiInput) {
        match input {
            UiInput::Pointer(pointer) => {
                self.pointer_position = Some((pointer.column, pointer.row));
                if matches!(pointer.kind, PointerKind::Move) {
                    self.hovered = self.hover_target(*pointer);
                }
            }
            UiInput::HostFocusGained | UiInput::HostFocusLost => {
                self.pointer_position = None;
                self.hovered = None;
            }
            UiInput::KeyStroke(_)
            | UiInput::Key(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Resize { .. } => {}
        }
    }

    pub(super) fn reconcile_hover(&mut self) {
        if self.dragged_item.is_some() {
            return;
        }
        self.hovered = self.pointer_position.and_then(|(column, row)| {
            self.hover_target(PointerInput {
                column,
                row,
                kind: PointerKind::Move,
                extend_selection: false,
            })
        });
    }

    pub(super) fn hover_target(&self, pointer: PointerInput) -> Option<HitTarget> {
        let target = self.pointer_target_for_owner(pointer)?;
        if !self.selection_is_empty()
            && matches!(target, HitTarget::Thought(_) | HitTarget::Fold(_, _))
        {
            return None;
        }
        Some(target)
    }
}
