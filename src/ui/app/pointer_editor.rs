//! Editor cell mapping shared by durable Edit and transient Compose pointers.

use crate::{
    application::{Effect, InteractionMode},
    ports::editor::{EditCommand, SelectionGranularity},
    ports::environment::{Clock, IdGenerator},
    ui::{PointerInput, projection::BoardCellTarget},
};

use super::{BoardApp, UiKey};

impl BoardApp {
    pub(super) fn thought_cell_target(
        &self,
        thought_id: crate::domain::ThoughtId,
        pointer: PointerInput,
    ) -> Option<BoardCellTarget> {
        if matches!(
            self.state.mode,
            InteractionMode::Edit {
                thought_id: active_id
            } if active_id == thought_id
        ) {
            self.editor_cell(thought_id, pointer)
                .and_then(|(row, column)| self.editor_cell_target(row, column))
        } else {
            self.board_cell_target(thought_id, pointer)
        }
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
