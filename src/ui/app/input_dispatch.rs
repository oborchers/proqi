//! Primary board and editor input dispatch after modal handling.

use crate::{
    application::{DurabilityState, Effect, InteractionMode},
    ports::environment::{Clock, IdGenerator},
    ui::{PastePayload, ShortcutContext, ShortcutContextStack},
};

use super::{BoardApp, UiInput};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActiveInputOwner {
    Board,
    Compose,
    Edit,
    InsertionBoundary,
    Recovery,
    Direction,
    Search,
    Rename,
    Transfer,
    Invocation,
    InvocationQuery,
    GlobalDeliveryQuery,
    GlobalDeliveryDisposition,
    Commands,
    ReleaseHighlights,
    Update,
    Screenshot,
    Help,
}

impl ActiveInputOwner {
    pub(super) const fn context(self) -> ShortcutContext {
        match self {
            Self::Board => ShortcutContext::Board,
            Self::Compose => ShortcutContext::Compose,
            Self::Edit => ShortcutContext::Edit,
            Self::InsertionBoundary => ShortcutContext::InsertionBoundary,
            Self::Recovery => ShortcutContext::Recovery,
            Self::Direction => ShortcutContext::Direction,
            Self::Search => ShortcutContext::Search,
            Self::Rename => ShortcutContext::Rename,
            Self::Transfer => ShortcutContext::Transfer,
            Self::Invocation => ShortcutContext::Invocation,
            Self::InvocationQuery => ShortcutContext::InvocationQuery,
            Self::GlobalDeliveryQuery => ShortcutContext::GlobalDeliveryQuery,
            Self::GlobalDeliveryDisposition => ShortcutContext::GlobalDeliveryDisposition,
            Self::Commands => ShortcutContext::Commands,
            Self::ReleaseHighlights => ShortcutContext::ReleaseHighlights,
            Self::Update => ShortcutContext::Update,
            Self::Screenshot => ShortcutContext::Screenshot,
            Self::Help => ShortcutContext::Help,
        }
    }

    pub(super) const fn owns_modal_surface(self) -> bool {
        matches!(
            self,
            Self::Direction
                | Self::Search
                | Self::Rename
                | Self::Transfer
                | Self::Invocation
                | Self::InvocationQuery
                | Self::GlobalDeliveryQuery
                | Self::GlobalDeliveryDisposition
                | Self::Commands
                | Self::ReleaseHighlights
                | Self::Update
                | Self::Screenshot
                | Self::Help
        )
    }

    pub(super) const fn acknowledges_screenshot_auto_pause(self) -> bool {
        matches!(
            self,
            Self::Board | Self::Compose | Self::Edit | Self::InsertionBoundary
        )
    }
}

impl BoardApp {
    pub(super) fn resolve_shortcut_input(
        &self,
        contexts: &ShortcutContextStack,
        input: UiInput,
    ) -> Option<UiInput> {
        match input {
            UiInput::KeyStroke(stroke) => self
                .settings
                .shortcuts
                .dispatch(contexts, stroke)
                .map(|resolved| UiInput::Key(resolved.intention)),
            UiInput::Key(key) => Some(UiInput::Key(key)),
            input => Some(input),
        }
    }

    pub(super) fn active_input_route(&self) -> (ShortcutContextStack, ActiveInputOwner) {
        let owners = self.active_input_owners();
        let active = owners.last().copied().unwrap_or(ActiveInputOwner::Board);
        let contexts = ShortcutContextStack::new(owners.into_iter().map(ActiveInputOwner::context));
        (contexts, active)
    }

    fn active_input_owners(&self) -> Vec<ActiveInputOwner> {
        let base = match self.interaction_mode() {
            InteractionMode::Board => ActiveInputOwner::Board,
            InteractionMode::Compose => ActiveInputOwner::Compose,
            InteractionMode::Edit { .. } => ActiveInputOwner::Edit,
        };
        let mut owners = vec![base];
        if base == ActiveInputOwner::Board && self.insertion_focused() {
            owners.push(ActiveInputOwner::InsertionBoundary);
        }
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            owners.push(ActiveInputOwner::Recovery);
        }
        if self.submission_mode.is_some() {
            owners.push(ActiveInputOwner::Direction);
        }
        if self.search.is_some() {
            owners.push(ActiveInputOwner::Search);
        }
        if self.rename.is_some() {
            owners.push(ActiveInputOwner::Rename);
        }
        if self.transfer.is_some() {
            owners.push(ActiveInputOwner::Transfer);
        }
        if self.invocation_popup.is_some() {
            owners.push(if self.manual_invocation_query_active() {
                ActiveInputOwner::InvocationQuery
            } else {
                ActiveInputOwner::Invocation
            });
        }
        if let Some(owner) = self.global_delivery_input_owner() {
            owners.push(owner);
        }
        if self.palette.is_some() {
            owners.push(ActiveInputOwner::Commands);
        }
        if self.release_highlights.is_some() {
            owners.push(ActiveInputOwner::ReleaseHighlights);
        }
        if self.update_prompt.is_some() {
            owners.push(ActiveInputOwner::Update);
        }
        if self.screenshot.takeover.is_some() {
            owners.push(ActiveInputOwner::Screenshot);
        }
        if self.help {
            owners.push(ActiveInputOwner::Help);
        }
        owners
    }

    pub(super) fn handle_primary_input(
        &mut self,
        input: UiInput,
        preserves_handoff: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.invalidate_palette_selection_handoff(&input, preserves_handoff);
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
