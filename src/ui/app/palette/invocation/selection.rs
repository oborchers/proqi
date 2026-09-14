use crate::{
    application::InteractionMode,
    domain::ThoughtId,
    ui::app::{BoardApp, palette_handoff::EditorSelectionHandoff},
};

use super::{Applicability, CommandContext};

impl CommandContext {
    pub(super) fn copy_applicability(&self) -> Applicability {
        if matches!(self.board.mode, InteractionMode::Edit { .. }) {
            return Self::when(
                self.selection
                    .editor_handoff
                    .as_ref()
                    .is_some_and(EditorSelectionHandoff::has_selection),
                "Select text in the editor first",
            );
        }
        Self::when(self.board.focused, "No thought is focused")
    }

    pub(super) fn cut_applicability(&self) -> Applicability {
        let mutable = self.when_mutable();
        if !mutable.enabled || !matches!(self.board.mode, InteractionMode::Edit { .. }) {
            return mutable;
        }
        Self::when(
            self.selection
                .editor_handoff
                .as_ref()
                .is_some_and(EditorSelectionHandoff::has_selection),
            "Select text in the editor first",
        )
    }
}

pub(super) fn selection_is_contiguous(
    app: &BoardApp,
    selected: &[ThoughtId],
    count: usize,
) -> bool {
    if count < 2 || selected.len() != count {
        return false;
    }
    let live = app.state.board.live_thoughts();
    let positions = selected
        .iter()
        .filter_map(|id| live.iter().position(|thought| thought.id == *id))
        .collect::<Vec<_>>();
    positions.len() == selected.len()
        && positions
            .windows(2)
            .all(|pair| pair[1] == pair[0].saturating_add(1))
}
