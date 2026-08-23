//! Layout-derived mouse intentions shared with board and editor actions.

use crate::{
    application::{Action, Effect, InteractionMode},
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, PointerButton, PointerInput, PointerKind};
use crate::ui::HitTarget;

impl BoardApp {
    pub(super) fn handle_pointer(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match pointer.kind {
            PointerKind::Move => {
                self.hovered = self.hit(pointer);
                Vec::new()
            }
            PointerKind::ScrollUp => self.scroll_pointer(-1, ids, clock),
            PointerKind::ScrollDown => self.scroll_pointer(1, ids, clock),
            PointerKind::Down(PointerButton::Left) => self.pointer_down(pointer, ids, clock),
            PointerKind::Drag(PointerButton::Left) => self.pointer_drag(pointer, ids, clock),
            PointerKind::Up(PointerButton::Left) => self.pointer_up(ids, clock),
            PointerKind::Down(_) | PointerKind::Up(_) | PointerKind::Drag(_) => Vec::new(),
        }
    }

    fn pointer_down(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let target = self.hit(pointer);
        self.hovered = target;
        match target {
            Some(HitTarget::Thought(thought_id)) => {
                self.focus_and_place_cursor(thought_id, pointer, ids, clock)
            }
            Some(HitTarget::DragHandle(thought_id)) => {
                self.focus(thought_id);
                self.dragged_thought = Some(thought_id);
                self.drag_target = self.position_at(pointer.row);
                Vec::new()
            }
            Some(HitTarget::Overflow(thought_id)) => {
                self.focus(thought_id);
                self.collapse(ids, clock)
            }
            Some(HitTarget::Insert) => self.create(String::new(), ids, clock),
            Some(HitTarget::Commands) => {
                self.open_palette();
                Vec::new()
            }
            Some(HitTarget::Copy) => self.copy_active(ids),
            Some(HitTarget::Cut) => self.cut_active(ids, clock),
            Some(HitTarget::Delete) => self.delete(ids, clock),
            Some(HitTarget::Undo) => self.history(ids, clock, true),
            Some(HitTarget::Help) => {
                self.help = !self.help;
                Vec::new()
            }
            Some(HitTarget::Quit) => {
                self.request_quit();
                Vec::new()
            }
            Some(HitTarget::PaletteItem(index)) => self.execute_palette_index(index, ids, clock),
            Some(HitTarget::CloseOverlay) => {
                self.close_overlay();
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    fn pointer_drag(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.dragged_thought.is_some() {
            self.drag_target = self.position_at(pointer.row);
            return Vec::new();
        }
        let Some(HitTarget::Thought(thought_id)) = self.hit(pointer) else {
            return Vec::new();
        };
        if !matches!(self.state.mode, InteractionMode::Edit { thought_id: active } if active == thought_id)
        {
            return Vec::new();
        }
        let Some((row, column)) = self.editor_cell(thought_id, pointer) else {
            return Vec::new();
        };
        self.apply_edit(EditCommand::PointerDrag { row, column }, ids, clock)
    }

    fn pointer_up(&mut self, ids: &mut impl IdGenerator, clock: &impl Clock) -> Vec<Effect> {
        let thought_id = self.dragged_thought.take();
        let target = self.drag_target.take();
        match (thought_id, target) {
            (Some(thought_id), Some(to)) => {
                self.focus(thought_id);
                self.reorder_to(to, ids, clock)
            }
            _ => Vec::new(),
        }
    }

    fn focus_and_place_cursor(
        &mut self,
        thought_id: crate::domain::ThoughtId,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let cell = self.editor_cell(thought_id, pointer);
        self.focus(thought_id);
        self.enter_edit();
        let Some((row, column)) = cell else {
            return Vec::new();
        };
        self.apply_edit(EditCommand::PointerStart { row, column }, ids, clock)
    }

    fn scroll_pointer(
        &mut self,
        delta: isize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            let movement = if delta < 0 {
                crate::ports::editor::CursorMovement::VisualUp
            } else {
                crate::ports::editor::CursorMovement::VisualDown
            };
            return self.apply_edit(super::movement(movement), ids, clock);
        }
        self.move_focus(delta);
        Vec::new()
    }

    fn focus(&mut self, thought_id: crate::domain::ThoughtId) {
        let _effects = self.reduce(Action::FocusThought(Some(thought_id)));
    }

    fn reorder_to(
        &mut self,
        to: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        let current = self
            .state
            .board
            .live_thoughts()
            .iter()
            .position(|thought| thought.id == thought_id);
        if current == Some(to) {
            return Vec::new();
        }
        self.reduce(Action::MoveThought {
            operation_id: ids.operation_id(),
            thought_id,
            to,
            at: clock.now(),
        })
    }

    fn editor_cell(
        &self,
        thought_id: crate::domain::ThoughtId,
        pointer: PointerInput,
    ) -> Option<(u16, u16)> {
        let text = self.layout.as_ref()?.thought(thought_id)?.text_area;
        Some((
            pointer.row.saturating_sub(text.y),
            pointer.column.saturating_sub(text.x),
        ))
    }

    fn hit(&self, pointer: PointerInput) -> Option<HitTarget> {
        self.layout
            .as_ref()
            .and_then(|layout| layout.hit_test(pointer.column, pointer.row))
    }

    fn position_at(&self, row: u16) -> Option<usize> {
        self.layout
            .as_ref()
            .and_then(|layout| layout.insertion_index_at(row))
    }
}
