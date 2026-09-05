//! Primary board and editor input dispatch after modal handling.

use crate::{
    application::{DurabilityState, Effect, InteractionMode},
    ports::environment::{Clock, IdGenerator},
    ui::{PastePayload, ShortcutContext, ShortcutContextStack, UiInput},
};

use super::BoardApp;

impl BoardApp {
    pub(super) fn resolve_shortcut_input(&self, input: UiInput) -> Option<UiInput> {
        let contexts = self.active_shortcut_contexts();
        match input {
            UiInput::KeyStroke(stroke) => self
                .shortcut_registry
                .dispatch(&contexts, stroke)
                .map(|resolved| UiInput::Key(resolved.intention)),
            UiInput::Key(key) => Some(UiInput::Key(
                self.shortcut_registry
                    .normalize_existing_intention(&contexts, key),
            )),
            input => Some(input),
        }
    }

    pub(crate) fn active_shortcut_contexts(&self) -> ShortcutContextStack {
        let base = match self.interaction_mode() {
            InteractionMode::Board => ShortcutContext::Board,
            InteractionMode::Compose => ShortcutContext::Compose,
            InteractionMode::Edit { .. } => ShortcutContext::Edit,
        };
        let mut contexts = vec![base];
        if matches!(base, ShortcutContext::Board) && self.insertion_focused() {
            contexts.push(ShortcutContext::InsertionBoundary);
        }
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            contexts.push(ShortcutContext::Recovery);
        }
        if self.submission_mode.is_some() {
            contexts.push(ShortcutContext::Direction);
        }
        if self.search.is_some() {
            contexts.push(ShortcutContext::Search);
        }
        if self.rename.is_some() {
            contexts.push(ShortcutContext::Rename);
        }
        if self.transfer.is_some() {
            contexts.push(ShortcutContext::Transfer);
        }
        if self.invocation_popup.is_some() {
            contexts.push(if self.manual_invocation_query_active() {
                ShortcutContext::InvocationQuery
            } else {
                ShortcutContext::Invocation
            });
        }
        if self.palette.is_some() {
            contexts.push(ShortcutContext::Commands);
        }
        if self.release_highlights.is_some() {
            contexts.push(ShortcutContext::ReleaseHighlights);
        }
        if self.update_prompt.is_some() {
            contexts.push(ShortcutContext::Update);
        }
        if self.screenshot.takeover.is_some() {
            contexts.push(ShortcutContext::Screenshot);
        }
        if self.help {
            contexts.push(ShortcutContext::Help);
        }
        ShortcutContextStack::new(contexts)
    }

    pub(super) fn handle_primary_input(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.invalidate_palette_selection_handoff(&input);
        match input {
            UiInput::HostFocusGained => Self::discover_agents(),
            UiInput::KeyStroke(_) | UiInput::HostFocusLost => Vec::new(),
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
