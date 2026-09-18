//! Owner-aware pointer identities shared by hover and activation.

use crate::ui::{HitTarget, PointerInput, projection::BoardCellTarget};

use super::BoardApp;

impl BoardApp {
    pub(super) fn pointer_target_for_owner(&self, pointer: PointerInput) -> Option<HitTarget> {
        let target = self.pointer_target(pointer)?;
        self.active_input_route()
            .1
            .admits_pointer_target(target)
            .then_some(target)
    }

    pub(super) fn pointer_target(&self, pointer: PointerInput) -> Option<HitTarget> {
        match self.hit(pointer)? {
            HitTarget::Thought(thought_id) => match self.thought_cell_target(thought_id, pointer) {
                Some(BoardCellTarget::Fold {
                    annotation_index, ..
                }) => Some(HitTarget::Fold(thought_id, annotation_index)),
                _ => Some(HitTarget::Thought(thought_id)),
            },
            target => Some(target),
        }
    }
}
