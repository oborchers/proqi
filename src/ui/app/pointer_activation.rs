//! Suppression for repeated pointer activation after a transient overlay closes.

use crate::domain::Timestamp;

use super::{BoardApp, PointerInput, PointerKind, pointer::MULTI_CLICK_MILLIS};

#[derive(Clone, Copy)]
pub(super) struct OverlayActivation {
    column: u16,
    row: u16,
    at: Timestamp,
}

impl BoardApp {
    pub(super) fn reset_overlay_activation_for_input(&mut self, input: &crate::ui::UiInput) {
        if !matches!(
            input,
            crate::ui::UiInput::Pointer(PointerInput {
                kind: PointerKind::Down(_) | PointerKind::Up(_),
                ..
            })
        ) {
            self.overlay_activation = None;
        }
    }

    pub(super) fn begin_overlay_activation(&mut self, pointer: PointerInput, at: Timestamp) {
        self.overlay_activation = Some(OverlayActivation {
            column: pointer.column,
            row: pointer.row,
            at,
        });
    }

    pub(super) fn consume_repeated_overlay_activation(
        &mut self,
        pointer: PointerInput,
        now: Timestamp,
    ) -> bool {
        let Some(previous) = self.overlay_activation else {
            return false;
        };
        let repeated = previous.column.abs_diff(pointer.column) <= 1
            && previous.row.abs_diff(pointer.row) <= 1
            && now
                .as_millis()
                .checked_sub(previous.at.as_millis())
                .is_some_and(|elapsed| (0..=MULTI_CLICK_MILLIS).contains(&elapsed));
        if !repeated {
            self.overlay_activation = None;
        }
        repeated
    }
}
