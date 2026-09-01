//! Primary board and editor input dispatch after modal handling.

use crate::{
    application::{Effect, InteractionMode},
    ports::environment::{Clock, IdGenerator},
    ui::{PastePayload, UiInput},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn handle_primary_input(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.invalidate_palette_selection_handoff(&input);
        match input {
            UiInput::HostFocusGained => Self::discover_agents(),
            UiInput::HostFocusLost => Vec::new(),
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
                self.edit_boundary = None;
                Vec::new()
            }
            UiInput::Pointer(pointer) => self.handle_pointer(pointer, ids, clock),
            UiInput::Paste(content) => {
                let effects = self.paste_payload(PastePayload::text(content), ids, clock);
                self.refresh_invocation_popup_after_input(effects)
            }
            UiInput::PasteAnnotated(payload) => {
                let effects = self.paste_payload(payload, ids, clock);
                self.refresh_invocation_popup_after_input(effects)
            }
            UiInput::Key(key) => match self.interaction_mode() {
                InteractionMode::Board => self.handle_board_key(key, ids, clock),
                InteractionMode::Compose => {
                    let effects = self.handle_compose_key(key, ids, clock);
                    self.refresh_invocation_popup();
                    effects
                }
                InteractionMode::Edit { .. } => {
                    let effects = self.handle_edit_key(key, ids, clock);
                    self.refresh_invocation_popup_after_input(effects)
                }
            },
        }
    }
}
