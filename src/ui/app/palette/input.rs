//! Modal Commands input, query editing, scrolling, and rendered activation proof.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
    ui::{PointerButton, PointerKind},
};

use super::{BoardApp, QueryEditor, UiInput, UiKey};

impl BoardApp {
    pub(in crate::ui::app) fn palette_input_executes_quit(&self, input: &UiInput) -> bool {
        let Some(palette) = &self.palette else {
            return false;
        };
        let execution = match input {
            UiInput::Key(UiKey::Enter) if palette.selected_has_rendered_geometry() => {
                palette.execution_at(palette.selected)
            }
            UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) =>
            {
                let Some(crate::ui::HitTarget::PaletteItem(visible)) = self.hit(*pointer) else {
                    return false;
                };
                palette.execution_at(palette.scroll.saturating_add(visible))
            }
            _ => None,
        };
        super::command_requests_quit(execution)
    }

    pub(in crate::ui::app) fn handle_palette_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(key) => self.handle_palette_key(*key, ids, clock),
            input => self.handle_palette_non_key(input, ids, clock),
        }
    }

    fn handle_palette_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Shortcut(action) => return self.execute_bound_command(action, ids, clock),
            UiKey::Escape => self.close_overlay(),
            UiKey::Enter => {
                if !self.palette_selection_has_rendered_geometry() {
                    return Vec::new();
                }
                let selected = self.palette.as_ref().map_or(0, |palette| palette.selected);
                return self.execute_palette_index(selected, ids, clock);
            }
            UiKey::Backspace => {
                if let Some(palette) = &mut self.palette {
                    palette.query.backspace();
                    palette.invalidate_rendered_geometry();
                    palette.clamp();
                }
            }
            UiKey::FastNavigation { direction, .. } => self.move_palette(direction.delta()),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualUp,
                ..
            } => self.move_palette(-1),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                ..
            } => self.move_palette(1),
            UiKey::Move {
                movement,
                extend_selection,
            } => {
                if let Some(palette) = &mut self.palette {
                    palette
                        .query
                        .move_cursor_with_selection(movement, extend_selection);
                }
            }
            UiKey::Delete | UiKey::ModifiedDelete => {
                if let Some(palette) = &mut self.palette {
                    palette.query.delete();
                    palette.invalidate_rendered_geometry();
                    palette.clamp();
                }
            }
            UiKey::Character(character) if !character.is_control() => {
                return self.update_palette_query(|query| query.insert_char(character));
            }
            UiKey::UnmodifiedSpace => {
                return self.update_palette_query(|query| query.insert_char(' '));
            }
            UiKey::SelectAll => {
                if let Some(palette) = &mut self.palette {
                    palette.query.select_all();
                }
            }
            UiKey::Undo => {
                if let Some(palette) = &mut self.palette {
                    palette.query.undo();
                    palette.invalidate_rendered_geometry();
                    palette.clamp();
                }
            }
            UiKey::Redo => {
                if let Some(palette) = &mut self.palette {
                    palette.query.redo();
                    palette.invalidate_rendered_geometry();
                    palette.clamp();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_palette_non_key(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Pointer(pointer) => match pointer.kind {
                crate::ui::PointerKind::ScrollUp => {
                    self.move_palette(-1);
                    Vec::new()
                }
                crate::ui::PointerKind::ScrollDown => {
                    self.move_palette(1);
                    Vec::new()
                }
                _ => self.handle_pointer(*pointer, ids, clock),
            },
            UiInput::Paste(value) => self.update_palette_query(|query| query.paste(value)),
            UiInput::PasteAnnotated(payload) => {
                self.update_palette_query(|query| query.paste(&payload.content))
            }
            UiInput::Resize { .. } => {
                if let Some(palette) = &mut self.palette {
                    palette.invalidate_rendered_geometry();
                }
                self.layout = None;
                Vec::new()
            }
            UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => Vec::new(),
        }
    }

    pub(super) fn update_palette_query(
        &mut self,
        update: impl FnOnce(&mut QueryEditor),
    ) -> Vec<Effect> {
        if let Some(palette) = &mut self.palette {
            update(&mut palette.query);
            palette.selected = 0;
            palette.scroll = 0;
            palette.invalidate_rendered_geometry();
            palette.clamp();
        }
        self.layout = None;
        Vec::new()
    }

    fn palette_selection_has_rendered_geometry(&self) -> bool {
        let Some(palette) = &self.palette else {
            return false;
        };
        palette.selected_has_rendered_geometry()
    }

    fn move_palette(&mut self, delta: isize) {
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(palette) = &mut self.palette else {
            return;
        };
        let previous_scroll = palette.scroll;
        palette.move_selection(delta);
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
        if palette.scroll != previous_scroll {
            palette.invalidate_rendered_geometry();
            self.layout = None;
        }
    }

    pub(in crate::ui::app) fn ensure_palette_visible(&mut self, visible: usize) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.clamp();
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
    }

    pub(in crate::ui::app) fn record_palette_rendered_geometry(
        &mut self,
        interactivity: Vec<bool>,
    ) {
        if let Some(palette) = &mut self.palette {
            palette.record_rendered_geometry(interactivity);
        }
    }
}
