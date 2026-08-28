//! Contextual-help navigation kept separate from board mutations.

use crate::application::{Effect, InteractionMode};
use crate::ports::editor::CursorMovement;

use super::{BoardApp, HitTarget, PointerButton, PointerKind, UiInput, UiKey};

impl BoardApp {
    pub(super) fn toggle_help(&mut self) -> Vec<Effect> {
        if !self.help {
            self.deactivate_range_latch();
        }
        self.help = !self.help;
        Vec::new()
    }

    pub(super) fn handle_help_input(&mut self, input: &UiInput) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.close_help(),
            UiInput::Key(UiKey::Character(character))
                if *character == self.settings.keybindings.help =>
            {
                self.close_help();
            }
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            }) => self.help_scroll = self.help_scroll.saturating_sub(1),
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualDown,
                ..
            }) => {
                self.help_scroll = self
                    .help_scroll
                    .saturating_add(1)
                    .min(self.help_max_scroll());
            }
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
                self.help_scroll = 0;
            }
            UiInput::Pointer(pointer) => self.handle_help_pointer(*pointer),
            UiInput::HostFocusGained
            | UiInput::Key(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_) => {}
        }
        Vec::new()
    }

    fn handle_help_pointer(&mut self, pointer: super::PointerInput) {
        match pointer.kind {
            PointerKind::ScrollUp => self.help_scroll = self.help_scroll.saturating_sub(1),
            PointerKind::ScrollDown => {
                self.help_scroll = self
                    .help_scroll
                    .saturating_add(1)
                    .min(self.help_max_scroll());
            }
            PointerKind::Down(PointerButton::Left)
                if self
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.hit_test(pointer.column, pointer.row))
                    == Some(HitTarget::CloseOverlay) =>
            {
                self.close_help();
            }
            PointerKind::Down(_)
            | PointerKind::Up(_)
            | PointerKind::Drag(_)
            | PointerKind::Move => {}
        }
    }

    fn close_help(&mut self) {
        self.help = false;
        self.help_scroll = 0;
    }

    fn help_max_scroll(&self) -> usize {
        let Some(overlay) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
        else {
            return 0;
        };
        let item_count = if matches!(self.interaction_mode(), InteractionMode::Edit { .. }) {
            7
        } else {
            17 + usize::from(self.supports_submission()) * 2
        };
        let columns = usize::from(overlay.area.width >= 48) + 1;
        let rows = item_count.div_ceil(columns);
        rows.saturating_sub(usize::from(overlay.area.height.saturating_sub(2)))
    }

    pub(in crate::ui) const fn help_scroll(&self) -> usize {
        self.help_scroll
    }
}
