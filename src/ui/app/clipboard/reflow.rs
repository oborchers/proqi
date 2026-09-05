//! Explicit reflow paste policy layered over the exact clipboard transaction.

use crate::{
    application::{Effect, InteractionMode},
    ports::environment::{Clock, IdGenerator},
    ui::{PastePayload, annotations::PasteReflow},
};

use super::super::{
    BoardApp, palette::command::Command, palette_handoff::EditorSelectionHandoff,
    pending_types::ClipboardPasteMode,
};

impl BoardApp {
    pub(in crate::ui::app) fn execute_palette_paste(
        &mut self,
        command: Command,
        handoff: Option<EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(handoff) = handoff {
            if !self.palette_handoff_is_current(&handoff) {
                self.set_warning("thought changed before paste was chosen");
                return effects;
            }
            if !matches!(self.state.mode, InteractionMode::Edit { .. }) {
                effects.extend(self.expand_and_enter_edit(ids, clock));
            }
            self.restore_palette_selection_handoff(Some(handoff));
        }
        effects.extend(if command == Command::PasteReflow {
            self.read_clipboard_reflow(ids)
        } else {
            self.read_clipboard(ids)
        });
        effects
    }

    pub(in crate::ui::app) fn read_clipboard_reflow(
        &mut self,
        ids: &mut impl IdGenerator,
    ) -> Vec<Effect> {
        self.read_clipboard_with_mode(ids, ClipboardPasteMode::Reflow)
    }

    pub(super) fn paste_reflow_result(
        &mut self,
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match payload.reflow() {
            Ok(PasteReflow::Changed(reflowed)) => {
                let effects = self.paste_payload(reflowed, ids, clock);
                if !effects.is_empty() {
                    self.set_success("pasted and reflowed");
                }
                effects
            }
            Ok(PasteReflow::Unchanged) => {
                let effects = self.paste_payload(payload, ids, clock);
                if !effects.is_empty() {
                    self.set_warning("pasted exactly; nothing to reflow");
                }
                effects
            }
            Ok(PasteReflow::Empty) => {
                self.set_warning("nothing remained after reflow");
                Vec::new()
            }
            Err(()) => {
                let effects = self.paste_payload(payload, ids, clock);
                if !effects.is_empty() {
                    self.set_warning("could not reflow; pasted exactly");
                }
                effects
            }
        }
    }
}
