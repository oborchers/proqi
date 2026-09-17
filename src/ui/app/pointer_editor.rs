//! Editor cell mapping shared by durable Edit and transient Compose pointers.

use crate::{
    application::{Effect, InteractionMode},
    domain::{TextPosition, ThoughtId},
    ports::editor::{EditCommand, SelectionGranularity},
    ports::environment::{Clock, IdGenerator},
    ui::{PointerInput, projection::BoardCellTarget},
};

use super::{BoardApp, UiKey};

impl BoardApp {
    pub(super) fn focus_and_place_cursor(
        &mut self,
        thought_id: ThoughtId,
        pointer: PointerInput,
        click_count: u8,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Edit { thought_id: active } if active == thought_id)
        {
            let target = self
                .editor_cell(thought_id, pointer)
                .and_then(|(row, column)| self.editor_cell_target(row, column));
            self.focus(thought_id);
            self.enter_edit();
            let Some(target) = target else {
                return Vec::new();
            };
            self.apply_board_cell_target(target, pointer, click_count);
            return Vec::new();
        }
        let target = self.board_cell_target(thought_id, pointer);
        self.focus(thought_id);
        let effects = self.expand_and_enter_edit(ids, clock);
        if let Some(target) = target {
            self.apply_board_cell_target(target, pointer, click_count);
        }
        effects
    }

    fn apply_board_cell_target(
        &mut self,
        target: BoardCellTarget,
        pointer: PointerInput,
        click_count: u8,
    ) {
        match target {
            BoardCellTarget::Fold {
                canonical_start,
                canonical_end,
            } => self.set_editor_range(canonical_start, canonical_end),
            BoardCellTarget::Position(position) => {
                self.apply_pointer_start(position, pointer, click_count);
            }
        }
    }

    fn apply_pointer_start(
        &mut self,
        position: TextPosition,
        pointer: PointerInput,
        click_count: u8,
    ) {
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
    }

    fn board_cell_target(
        &self,
        thought_id: ThoughtId,
        pointer: PointerInput,
    ) -> Option<BoardCellTarget> {
        let frame = self.layout.as_ref()?;
        let layout = frame.thought(thought_id)?;
        let thought = self.frame_presentation.as_ref()?.thought(thought_id)?;
        let row = layout
            .content_row_offset
            .saturating_add(usize::from(pointer.row.saturating_sub(layout.text_area.y)));
        let column = pointer.column.saturating_sub(layout.text_area.x);
        crate::ui::projection::board_cell_target(
            &thought.canonical_content,
            &thought.presentation,
            layout.text_area.width,
            row,
            column,
        )
    }

    pub(super) fn pointer_insert(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Compose) {
            if self.compose_prompt_visible() {
                return self.new_thought(
                    super::creation::NewThoughtPlacement::Contextual,
                    ids,
                    clock,
                );
            }
            self.place_compose_cursor(pointer);
            Vec::new()
        } else {
            self.new_thought(
                super::creation::NewThoughtPlacement::DurableTail,
                ids,
                clock,
            )
        }
    }

    pub(super) fn pointer_exit_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Compose) {
            self.handle_compose_key(UiKey::Escape, ids, clock)
        } else {
            self.finish_edit(ids, clock)
        }
    }

    pub(super) fn editor_cell(
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

    pub(super) fn compose_cell(&self, pointer: PointerInput) -> Option<(u16, u16)> {
        let text = self.layout.as_ref()?.compose.as_ref()?.text_area;
        Some((
            pointer.row.saturating_sub(text.y),
            pointer.column.saturating_sub(text.x),
        ))
    }

    pub(super) fn place_compose_cursor(&mut self, pointer: PointerInput) {
        let Some((row, column)) = self.compose_cell(pointer) else {
            return;
        };
        let position = self
            .editor
            .as_ref()
            .map(|(_, editor)| editor.position_at_cell(row, column))
            .unwrap_or_default();
        self.apply_compose_transient(EditCommand::PointerStart {
            position,
            granularity: SelectionGranularity::Grapheme,
            extend_selection: pointer.extend_selection,
        });
    }
}
