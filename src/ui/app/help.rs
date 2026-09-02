//! Contextual-help navigation kept separate from board mutations.

use crate::application::Effect;
use crate::ui::ListNavigation;

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
            UiInput::Key(UiKey::FastNavigation { direction, .. }) => {
                self.scroll_help(*direction);
            }
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Previous) => {
                self.scroll_help_rows(-1);
            }
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Next) => {
                self.scroll_help_rows(1);
            }
            UiInput::Key(UiKey::Character(character))
                if *character == self.settings.keybindings.help =>
            {
                self.close_help();
            }
            UiInput::Key(UiKey::UnmodifiedSpace) if self.settings.keybindings.help == ' ' => {
                self.close_help();
            }
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
            }
            UiInput::Pointer(pointer) => self.handle_help_pointer(*pointer),
            UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Key(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_) => {}
        }
        Vec::new()
    }

    fn handle_help_pointer(&mut self, pointer: super::PointerInput) {
        match pointer.kind {
            PointerKind::ScrollUp => self.scroll_help_rows(-1),
            PointerKind::ScrollDown => self.scroll_help_rows(1),
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

    fn scroll_help(&mut self, direction: crate::ui::FastNavigation) {
        self.help_scroll = direction.move_scroll(self.help_scroll, self.help_max_scroll());
    }

    fn scroll_help_rows(&mut self, delta: isize) {
        self.help_scroll = self
            .help_scroll
            .saturating_add_signed(delta)
            .min(self.help_max_scroll());
    }

    fn help_max_scroll(&self) -> usize {
        let Some(overlay) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
        else {
            return 0;
        };
        let rows = crate::ui::shortcuts::row_count(self, overlay.area.width.saturating_sub(2));
        rows.saturating_sub(usize::from(overlay.area.height.saturating_sub(2)))
    }

    pub(super) fn clamp_help_scroll(&mut self) {
        self.help_scroll = self.help_scroll.min(self.help_max_scroll());
    }

    pub(in crate::ui) const fn help_scroll(&self) -> usize {
        self.help_scroll
    }

    pub(in crate::ui) fn help_overflow(&self) -> (bool, bool) {
        (
            self.help_scroll > 0,
            self.help_scroll < self.help_max_scroll(),
        )
    }
}
