//! Layout-derived mouse intentions shared with board and editor actions.

use crate::{
    application::{Action, Effect, InteractionMode},
    domain::{ThoughtId, Timestamp},
    ports::{
        editor::{EditCommand, SelectionGranularity},
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, PointerButton, PointerInput, PointerKind};
use crate::ui::HitTarget;

const MULTI_CLICK_MILLIS: i64 = 500;

#[derive(Clone, Copy)]
pub(super) struct PointerClick {
    thought_id: ThoughtId,
    column: u16,
    row: u16,
    at: Timestamp,
    count: u8,
}

impl BoardApp {
    pub(super) fn reset_pointer_click_for_input(&mut self, input: &crate::ui::UiInput) {
        if !matches!(
            input,
            crate::ui::UiInput::Pointer(PointerInput {
                kind: PointerKind::Down(PointerButton::Left)
                    | PointerKind::Up(PointerButton::Left)
                    | PointerKind::Move,
                ..
            })
        ) {
            self.pointer_click = None;
        }
    }

    pub(super) fn handle_recovery_pointer(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if !matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) {
            return Vec::new();
        }
        match self.hit(pointer) {
            Some(HitTarget::Retry) => self.retry_persistence(),
            Some(HitTarget::ExportRecovery) => self.export_recovery(ids, clock),
            Some(HitTarget::Help) => self.toggle_help(),
            _ => Vec::new(),
        }
    }

    pub(super) fn handle_pointer(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.edit_boundary = None;
        let mut effects = match pointer.kind {
            PointerKind::Down(_) | PointerKind::Drag(_) | PointerKind::Up(_) => {
                self.flush_pending_edit(ids, clock)
            }
            PointerKind::Move | PointerKind::ScrollUp | PointerKind::ScrollDown => Vec::new(),
        };
        effects.extend(match pointer.kind {
            PointerKind::Move => {
                self.hovered = self
                    .selection_is_empty()
                    .then(|| self.hit(pointer))
                    .flatten();
                Vec::new()
            }
            PointerKind::ScrollUp => self.scroll_pointer(-1),
            PointerKind::ScrollDown => self.scroll_pointer(1),
            PointerKind::Down(PointerButton::Left) => self.pointer_down(pointer, ids, clock),
            PointerKind::Drag(PointerButton::Left) => self.pointer_drag(pointer),
            PointerKind::Up(PointerButton::Left) => self.pointer_up(ids, clock),
            PointerKind::Down(_) | PointerKind::Up(_) | PointerKind::Drag(_) => Vec::new(),
        });
        effects
    }

    fn pointer_down(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let target = self.hit(pointer);
        self.hovered = target;
        if !matches!(target, Some(HitTarget::Thought(_))) {
            self.pointer_click = None;
        }
        match target {
            Some(HitTarget::Thought(thought_id)) => {
                self.handle_thought_pointer(thought_id, pointer, clock.now())
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
            Some(HitTarget::Insert) => {
                self.create(crate::ui::PastePayload::text(String::new()), ids, clock)
            }
            Some(HitTarget::Search) => {
                self.open_search();
                Vec::new()
            }
            Some(HitTarget::Commands) => {
                self.open_palette();
                Vec::new()
            }
            Some(HitTarget::RenameSession) => {
                self.begin_session_rename();
                Vec::new()
            }
            Some(HitTarget::CopySessionId) => self.copy_session_id(ids),
            Some(HitTarget::Copy) => self.copy_active(ids),
            Some(HitTarget::Cut) => self.cut_active(ids, clock),
            Some(HitTarget::Delete) => self.delete(ids, clock),
            Some(HitTarget::Select) => {
                self.toggle_selection();
                Vec::new()
            }
            Some(HitTarget::Deliver(direction, disposition)) => {
                self.deliver_to(direction, disposition, ids, clock)
            }
            Some(HitTarget::BeginDelivery(disposition)) => {
                self.begin_delivery(disposition, ids, clock)
            }
            Some(HitTarget::Undo) => self.history(ids, clock, true),
            Some(HitTarget::Help) => self.toggle_help(),
            Some(HitTarget::Quit) => {
                self.request_quit();
                Vec::new()
            }
            Some(HitTarget::ExitEdit) => self.finish_edit(ids, clock),
            Some(HitTarget::Retry) => self.retry_persistence(),
            Some(HitTarget::ExportRecovery) => self.export_recovery(ids, clock),
            Some(HitTarget::PaletteItem(index)) => {
                if self.search.is_some() {
                    self.execute_search_visible_index(index)
                } else if self.transfer.is_some() {
                    self.choose_transfer_visible(index, ids)
                } else {
                    self.execute_palette_visible_index(index, ids, clock)
                }
            }
            Some(HitTarget::CloseOverlay) => {
                self.close_overlay();
                Vec::new()
            }
            Some(HitTarget::Agent(_)) | None => {
                self.pointer_click = None;
                Vec::new()
            }
        }
    }

    fn handle_thought_pointer(
        &mut self,
        thought_id: ThoughtId,
        pointer: PointerInput,
        now: Timestamp,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Board)
            && (pointer.extend_selection || self.range_latched())
        {
            self.pointer_click = None;
            self.extend_range_to(thought_id);
            return Vec::new();
        }
        let click_count = self.register_text_click(thought_id, pointer, now);
        self.focus_and_place_cursor(thought_id, pointer, click_count)
    }

    fn pointer_drag(&mut self, pointer: PointerInput) -> Vec<Effect> {
        self.pointer_click = None;
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
        let position = self.projected_position_at_cell(row, column);
        self.apply_edit(EditCommand::PointerDrag { position });
        Vec::new()
    }

    fn pointer_up(&mut self, ids: &mut impl IdGenerator, clock: &impl Clock) -> Vec<Effect> {
        self.apply_edit(EditCommand::PointerEnd);
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
        click_count: u8,
    ) -> Vec<Effect> {
        let cell = self.editor_cell(thought_id, pointer);
        self.focus(thought_id);
        self.enter_edit();
        let Some((row, column)) = cell else {
            return Vec::new();
        };
        if self.select_fold_at_cell(thought_id, row, column) {
            return Vec::new();
        }
        let position = self.projected_position_at_cell(row, column);
        let granularity = match click_count {
            2 => SelectionGranularity::Word,
            3 => SelectionGranularity::LogicalLine,
            _ => SelectionGranularity::Grapheme,
        };
        self.apply_edit(EditCommand::PointerStart {
            position,
            granularity,
            extend_selection: pointer.extend_selection,
        });
        Vec::new()
    }

    fn register_text_click(
        &mut self,
        thought_id: ThoughtId,
        pointer: PointerInput,
        now: Timestamp,
    ) -> u8 {
        let repeated = self.pointer_click.is_some_and(|previous| {
            previous.thought_id == thought_id
                && previous.column.abs_diff(pointer.column) <= 1
                && previous.row.abs_diff(pointer.row) <= 1
                && now
                    .as_millis()
                    .checked_sub(previous.at.as_millis())
                    .is_some_and(|elapsed| (0..=MULTI_CLICK_MILLIS).contains(&elapsed))
        });
        let count = self.pointer_click.map_or(1, |previous| {
            if repeated && previous.count < 3 {
                previous.count + 1
            } else {
                1
            }
        });
        self.pointer_click = Some(PointerClick {
            thought_id,
            column: pointer.column,
            row: pointer.row,
            at: now,
            count,
        });
        count
    }

    fn scroll_pointer(&mut self, delta: isize) -> Vec<Effect> {
        if let Some((_, editor)) = &mut self.editor
            && matches!(self.state.mode, InteractionMode::Edit { .. })
        {
            editor.scroll_by(delta);
            return Vec::new();
        }
        let maximum = self
            .layout
            .as_ref()
            .map_or(0, |layout| layout.max_first_index);
        if delta > 0 {
            let can_scroll_current = self
                .layout
                .as_ref()
                .and_then(|layout| layout.thoughts.first())
                .is_some_and(|thought| thought.scrollable_hidden);
            if can_scroll_current {
                self.first_visible_row += 1;
            } else if self.first_visible < maximum {
                self.first_visible += 1;
                self.first_visible_row = 0;
            } else {
                return Vec::new();
            }
        } else if self.first_visible_row > 0 {
            self.first_visible_row -= 1;
        } else if self.first_visible > 0 {
            self.first_visible -= 1;
            self.first_visible_row = if self
                .state
                .board
                .live_thoughts()
                .get(self.first_visible)
                .is_some_and(|thought| {
                    thought.presentation != crate::domain::ThoughtPresentation::Collapsed
                }) {
                self.first_thought_row_count().saturating_sub(1)
            } else {
                0
            };
        } else {
            return Vec::new();
        }
        self.manual_board_scroll = true;
        self.layout = None;
        Vec::new()
    }

    fn first_thought_row_count(&self) -> usize {
        let Some(thought) = self
            .state
            .board
            .live_thoughts()
            .get(self.first_visible)
            .copied()
        else {
            return 1;
        };
        let content = self
            .presentation_for_render(thought.id)
            .map_or_else(|| thought.content.clone(), |view| view.content);
        crate::ports::text_layout::wrap_rows(&content, usize::from(self.viewport.width.max(1)))
            .len()
            .max(1)
    }

    fn focus(&mut self, thought_id: crate::domain::ThoughtId) {
        self.clear_range_for_focus_change();
        self.insertion_focus = super::InsertionFocus::Inactive;
        self.manual_board_scroll = false;
        self.first_visible_row = 0;
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
