//! Suppression for repeated pointer activation after a transient overlay closes.

use crate::domain::Timestamp;
use ratatui_core::layout::Rect;

use super::{BoardApp, PointerButton, PointerInput, PointerKind, pointer::MULTI_CLICK_MILLIS};

#[derive(Clone, Copy)]
pub(super) struct OverlayActivation {
    column: u16,
    row: u16,
    at: Timestamp,
    area: Rect,
}

impl BoardApp {
    pub(super) fn reset_overlay_activation_for_input(
        &mut self,
        input: &crate::ui::UiInput,
        now: Timestamp,
    ) {
        let preserves = match input {
            crate::ui::UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) =>
            {
                true
            }
            crate::ui::UiInput::Pointer(pointer)
                if matches!(
                    pointer.kind,
                    PointerKind::Up(PointerButton::Left) | PointerKind::Move
                ) =>
            {
                self.overlay_activation
                    .is_some_and(|activation| activation.matches(*pointer, now))
            }
            crate::ui::UiInput::Key(_)
            | crate::ui::UiInput::Pointer(_)
            | crate::ui::UiInput::Paste(_)
            | crate::ui::UiInput::PasteAnnotated(_)
            | crate::ui::UiInput::Resize { .. }
            | crate::ui::UiInput::HostFocusGained
            | crate::ui::UiInput::HostFocusLost => false,
        };
        if !preserves {
            self.overlay_activation = None;
        }
    }

    pub(super) fn begin_overlay_activation(&mut self, pointer: PointerInput, at: Timestamp) {
        let Some(area) = self.layout.as_ref().map(|layout| layout.area) else {
            self.overlay_activation = None;
            return;
        };
        self.overlay_activation = Some(OverlayActivation {
            column: pointer.column,
            row: pointer.row,
            at,
            area,
        });
    }

    pub(super) fn reset_overlay_activation_for_geometry(&mut self, area: Rect) {
        if self
            .overlay_activation
            .is_some_and(|activation| activation.area != area)
        {
            self.overlay_activation = None;
        }
    }

    pub(super) fn consume_repeated_overlay_activation(
        &mut self,
        pointer: PointerInput,
        now: Timestamp,
    ) -> bool {
        let Some(previous) = self.overlay_activation else {
            return false;
        };
        let repeated = previous.matches(pointer, now);
        if !repeated {
            self.overlay_activation = None;
        }
        repeated
    }
}

impl OverlayActivation {
    fn matches(self, pointer: PointerInput, now: Timestamp) -> bool {
        self.column.abs_diff(pointer.column) <= 1
            && self.row.abs_diff(pointer.row) <= 1
            && now
                .as_millis()
                .checked_sub(self.at.as_millis())
                .is_some_and(|elapsed| (0..=MULTI_CLICK_MILLIS).contains(&elapsed))
    }
}
