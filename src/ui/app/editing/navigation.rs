//! Modal-aware normalization of editor and board navigation intentions.

use crate::{
    application::InteractionMode,
    ui::{UiInput, UiKey},
};

use super::super::BoardApp;

impl BoardApp {
    pub(in crate::ui::app) fn resolve_edit_navigation(&self, input: UiInput) -> UiInput {
        if let UiInput::Key(UiKey::FastNavigation {
            direction,
            extend_selection,
        }) = input
        {
            if self.modal_surface_open() {
                return input;
            }
            let movement = if matches!(
                self.interaction_mode(),
                InteractionMode::Edit { .. } | InteractionMode::Compose
            ) {
                direction.editor_movement()
            } else {
                direction.board_movement()
            };
            return UiInput::Key(UiKey::Move {
                movement,
                extend_selection,
            });
        }
        let UiInput::Key(UiKey::EditNavigation {
            editor_movement,
            board_movement,
        }) = input
        else {
            return input;
        };
        let movement = if !self.modal_surface_open()
            && matches!(self.interaction_mode(), InteractionMode::Edit { .. })
        {
            editor_movement
        } else {
            board_movement
        };
        UiInput::Key(UiKey::Move {
            movement,
            extend_selection: false,
        })
    }

    fn modal_surface_open(&self) -> bool {
        self.help
            || self.screenshot.takeover.is_some()
            || self.update_prompt.is_some()
            || self.release_highlights.is_some()
            || self.palette.is_some()
            || self.global_delivery.is_some()
            || self.invocation_popup.is_some()
            || self.transfer.is_some()
            || self.rename.is_some()
            || self.search.is_some()
            || self.submission_mode.is_some()
    }
}
