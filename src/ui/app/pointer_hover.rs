//! Passive pointer emphasis derived from the current rendered frame.

use crate::ui::{HitTarget, PointerInput, PointerKind, projection::BoardCellTarget};

use super::{BoardApp, UiInput, input_dispatch::ActiveInputOwner};

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
        if !self.selection_is_empty() && matches!(target, HitTarget::Thought(_)) {
            return None;
        }
        Some(target)
    }

    pub(super) fn pointer_target_for_owner(&self, pointer: PointerInput) -> Option<HitTarget> {
        let target = self.pointer_target(pointer)?;
        match self.active_input_route().1 {
            ActiveInputOwner::Direction => {
                matches!(target, HitTarget::Deliver(_, _)).then_some(target)
            }
            ActiveInputOwner::Recovery => matches!(
                target,
                HitTarget::Retry | HitTarget::ExportRecovery | HitTarget::Help
            )
            .then_some(target),
            ActiveInputOwner::Board
            | ActiveInputOwner::Compose
            | ActiveInputOwner::Edit
            | ActiveInputOwner::InsertionBoundary
            | ActiveInputOwner::Search
            | ActiveInputOwner::Rename
            | ActiveInputOwner::Transfer
            | ActiveInputOwner::Invocation
            | ActiveInputOwner::InvocationQuery
            | ActiveInputOwner::GlobalDeliveryQuery
            | ActiveInputOwner::GlobalDeliveryDisposition
            | ActiveInputOwner::Commands
            | ActiveInputOwner::ReleaseHighlights
            | ActiveInputOwner::Update
            | ActiveInputOwner::Screenshot
            | ActiveInputOwner::Help => Some(target),
        }
    }

    pub(super) fn pointer_target(&self, pointer: PointerInput) -> Option<HitTarget> {
        match self.hit(pointer)? {
            HitTarget::Thought(thought_id) => match self.board_cell_target(thought_id, pointer) {
                Some(BoardCellTarget::Fold {
                    annotation_index, ..
                }) => Some(HitTarget::Fold(thought_id, annotation_index)),
                _ => Some(HitTarget::Thought(thought_id)),
            },
            target => Some(target),
        }
    }
}
