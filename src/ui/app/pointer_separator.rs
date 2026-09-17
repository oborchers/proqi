//! Pointer focus, range selection, and drag ownership for separators.

use crate::{
    application::Effect,
    domain::{BoardItemId, SeparatorId},
    ui::{HitTarget, PointerInput},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn reconcile_board_hover(&mut self, target: Option<HitTarget>) {
        let passive_control = matches!(
            target,
            Some(
                HitTarget::Separator(_)
                    | HitTarget::SeparatorDragHandle(_)
                    | HitTarget::ThoughtName(_)
                    | HitTarget::CommitThoughtName
                    | HitTarget::CancelThoughtName
            )
        );
        self.hovered = (self.selection_is_empty() || passive_control)
            .then_some(target)
            .flatten();
    }

    pub(super) fn handle_separator_pointer(
        &mut self,
        separator_id: SeparatorId,
        pointer: PointerInput,
        drag: bool,
    ) -> Vec<Effect> {
        self.pointer_click = None;
        let item_id = BoardItemId::Separator(separator_id);
        if drag {
            self.focus_item(item_id);
            self.dragged_item = Some(item_id);
            self.drag_target = self.position_at(pointer.row);
        } else if pointer.extend_selection || self.range_latched() {
            self.extend_range_to(item_id);
        } else {
            self.focus_item(item_id);
        }
        Vec::new()
    }
}
