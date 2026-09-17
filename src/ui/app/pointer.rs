//! Layout-derived mouse intentions shared with board and editor actions.

use crate::{
    application::{Action, Effect, InteractionMode},
    domain::{ThoughtId, Timestamp},
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, PointerButton, PointerInput, PointerKind, pending_types::EditFlush};
use crate::ui::HitTarget;

pub(super) const MULTI_CLICK_MILLIS: i64 = 500;

#[derive(Clone, Copy)]
pub(super) struct PointerClick {
    thought_id: ThoughtId,
    column: u16,
    row: u16,
    at: Timestamp,
    pub(super) count: u8,
}

impl BoardApp {
    pub(super) fn reset_pointer_click_for_input(&mut self, input: &super::UiInput) {
        if !matches!(
            input,
            super::UiInput::Pointer(PointerInput {
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
        if self.submission_mode.is_some() {
            return self.handle_submission_pointer(pointer, ids, clock);
        }
        if matches!(pointer.kind, PointerKind::Down(PointerButton::Left))
            && self.consume_repeated_overlay_activation(pointer, clock.now())
        {
            return Vec::new();
        }
        let flush = match pointer.kind {
            PointerKind::Down(_) | PointerKind::Drag(_) | PointerKind::Up(_) => {
                self.flush_edit_boundary(ids, clock)
            }
            PointerKind::Move | PointerKind::ScrollUp | PointerKind::ScrollDown => {
                EditFlush::Complete(Vec::new())
            }
        };
        let mut effects = match flush {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        effects.extend(match pointer.kind {
            PointerKind::Move => {
                let target = self.hit(pointer);
                self.reconcile_board_hover(target);
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

    fn handle_submission_pointer(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let target = self.hit(pointer);
        if matches!(pointer.kind, PointerKind::Move) {
            self.hovered = target;
            return Vec::new();
        }
        let Some(HitTarget::Deliver(direction, disposition)) = target else {
            return Vec::new();
        };
        if !matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) {
            return Vec::new();
        }
        self.deliver_to(direction, disposition, ids, clock)
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
            Some(HitTarget::ThoughtName(thought_id)) => {
                self.begin_thought_rename_from_pointer(thought_id, pointer, ids, clock)
            }
            Some(HitTarget::Thought(thought_id)) => {
                self.handle_thought_pointer(thought_id, pointer, ids, clock)
            }
            Some(HitTarget::DragHandle(thought_id)) => {
                self.begin_thought_drag(thought_id, pointer.row, ids, clock)
            }
            Some(HitTarget::Separator(separator_id)) => {
                self.handle_separator_pointer(separator_id, pointer, false)
            }
            Some(HitTarget::SeparatorDragHandle(separator_id)) => {
                self.handle_separator_pointer(separator_id, pointer, true)
            }
            Some(HitTarget::Overflow(thought_id)) => {
                self.focus(thought_id);
                self.expand_thought(thought_id, ids, clock)
            }
            Some(HitTarget::Insert) => self.pointer_insert(pointer, ids, clock),
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
            Some(HitTarget::Redo) => self.history(ids, clock, false),
            Some(HitTarget::Help) => self.toggle_help(),
            Some(HitTarget::Quit) => {
                self.request_quit();
                Vec::new()
            }
            Some(HitTarget::ExitEdit) => self.pointer_exit_edit(ids, clock),
            Some(HitTarget::CommitThoughtName | HitTarget::CancelThoughtName) => Vec::new(),
            Some(HitTarget::Retry) => self.retry_persistence(),
            Some(HitTarget::ExportRecovery) => self.export_recovery(ids, clock),
            Some(HitTarget::PaletteItem(index)) => {
                self.activate_palette_item(index, pointer, ids, clock)
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

    fn begin_thought_drag(
        &mut self,
        thought_id: ThoughtId,
        row: u16,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let expand = self.activation_needs_expansion(thought_id);
        self.focus(thought_id);
        if expand {
            return self.expand_thought(thought_id, ids, clock);
        }
        self.dragged_item = Some(crate::domain::BoardItemId::Thought(thought_id));
        self.drag_target = self.position_at(row);
        Vec::new()
    }

    fn activate_palette_item(
        &mut self,
        index: usize,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.begin_overlay_activation(pointer, clock.now());
        if self.screenshot.takeover.is_some() {
            self.screenshot.takeover_selected = index.min(1);
            self.choose_screenshot_takeover(ids)
        } else if self.search.is_some() {
            self.execute_search_visible_index(index)
        } else if self.transfer.is_some() {
            self.choose_transfer_visible(index, ids)
        } else if self.global_delivery.is_some() {
            self.choose_global_delivery_visible(index, ids, clock)
        } else if self.execute_invocation_visible_index(index) {
            Vec::new()
        } else {
            self.execute_palette_visible_index(index, ids, clock)
        }
    }

    fn handle_thought_pointer(
        &mut self,
        thought_id: ThoughtId,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Board)
            && (pointer.extend_selection || self.range_latched())
        {
            self.pointer_click = None;
            self.extend_range_to(crate::domain::BoardItemId::Thought(thought_id));
            return Vec::new();
        }
        let click_count = self.register_text_click(thought_id, pointer, clock.now());
        self.focus_and_place_cursor(thought_id, pointer, click_count, ids, clock)
    }

    fn pointer_drag(&mut self, pointer: PointerInput) -> Vec<Effect> {
        self.pointer_click = None;
        if self.dragged_item.is_some() {
            self.drag_target = self.position_at(pointer.row);
            return Vec::new();
        }
        if self.hit(pointer) == Some(HitTarget::Insert)
            && matches!(self.state.mode, InteractionMode::Compose)
        {
            if let Some((row, column)) = self.compose_cell(pointer) {
                let position = self
                    .editor
                    .as_ref()
                    .map(|(_, editor)| editor.position_at_cell(row, column))
                    .unwrap_or_default();
                self.apply_compose_transient(EditCommand::PointerDrag { position });
            }
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
        if matches!(self.state.mode, InteractionMode::Compose) {
            self.apply_compose_transient(EditCommand::PointerEnd);
        } else {
            self.apply_edit(EditCommand::PointerEnd);
        }
        let item_id = self.dragged_item.take();
        let target = self.drag_target.take();
        match (item_id, target) {
            (Some(item_id), Some(to)) => {
                self.focus_item(item_id);
                self.reorder_to(to, ids, clock)
            }
            _ => Vec::new(),
        }
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
            && matches!(
                self.state.mode,
                InteractionMode::Compose | InteractionMode::Edit { .. }
            )
        {
            editor.scroll_by(delta);
            return Vec::new();
        }
        if self.layout.is_none() {
            return Vec::new();
        }
        let anchor = self
            .scroll_geometry
            .and_then(|geometry| geometry.neighbor(delta));
        let Some(anchor) = anchor else {
            return Vec::new();
        };
        self.scroll_board_to(anchor);
        Vec::new()
    }

    pub(super) fn focus(&mut self, thought_id: crate::domain::ThoughtId) {
        self.focus_item(crate::domain::BoardItemId::Thought(thought_id));
    }

    pub(super) fn focus_item(&mut self, item_id: crate::domain::BoardItemId) {
        self.clear_range_for_focus_change();
        self.insertion_focus = super::InsertionFocus::Inactive;
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        let _effects = self.reduce(Action::FocusItem(Some(item_id)));
    }

    fn reorder_to(
        &mut self,
        to: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(item_id) = self.state.focused_item else {
            return Vec::new();
        };
        let current = self
            .state
            .board
            .live_items()
            .iter()
            .position(|item| item.id() == item_id);
        if current == Some(to) {
            return Vec::new();
        }
        self.reduce(Action::MoveItem {
            operation_id: ids.operation_id(),
            item_id,
            to,
            at: clock.now(),
        })
    }

    pub(super) fn hit(&self, pointer: PointerInput) -> Option<HitTarget> {
        self.layout
            .as_ref()
            .and_then(|layout| layout.hit_test(pointer.column, pointer.row))
    }

    pub(super) fn position_at(&self, row: u16) -> Option<usize> {
        self.layout
            .as_ref()
            .and_then(|layout| layout.insertion_index_at(row))
    }
}
