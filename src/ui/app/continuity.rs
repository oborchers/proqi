//! Content-free UI checkpoint for exact input-lane process replacement.

use crate::{
    application::{DurabilityState, InteractionMode},
    ports::{
        editor::VisualCursorAffinity,
        runtime::{
            InputRecoveryBoardViewport, InputRecoveryEditorState, InputRecoveryMode,
            InputRecoveryScrollAnchor, InputRecoveryUiState,
        },
    },
    ui::layout::scroll::{BoardViewport, ScrollAnchor},
};

use super::{BoardApp, ComposePresentation, InsertionFocus};

const MAX_RECOVERY_SELECTIONS: usize = 1_024;
const MAX_RECOVERY_FOLDS: usize = 1_024;

impl BoardApp {
    pub(crate) fn input_recovery_state(&self) -> Option<InputRecoveryUiState> {
        if self.active_input_route().1.owns_modal_surface()
            || matches!(self.state.durability, DurabilityState::Failed { .. })
            || self.selection.len() > MAX_RECOVERY_SELECTIONS
            || self.expanded_folds.len() > MAX_RECOVERY_FOLDS
        {
            return None;
        }
        let mode = match self.state.mode {
            InteractionMode::Board => InputRecoveryMode::Board,
            InteractionMode::Compose => InputRecoveryMode::Compose,
            InteractionMode::Edit { thought_id } => InputRecoveryMode::Edit { thought_id },
        };
        let editor = self
            .editor_snapshot()
            .map(|snapshot| InputRecoveryEditorState {
                cursor: snapshot.cursor,
                selection_anchor: snapshot.selection_anchor,
                previous_row_affinity: snapshot.cursor_affinity
                    == VisualCursorAffinity::PreviousRow,
                scroll_row: snapshot.scroll_row,
            });
        Some(InputRecoveryUiState {
            mode,
            focused_item: self.state.focused_item,
            insertion_index: self.state.insertion_index,
            insertion_focused: self.insertion_focused(),
            compose_editor_visible: self.compose_editor_visible(),
            editor,
            selection: self.selection.recovery_state(),
            expanded_folds: self.expanded_folds.iter().copied().collect(),
            board_viewport: encode_viewport(self.board_viewport),
        })
    }

    pub(crate) fn restore_input_recovery_state(&mut self, state: InputRecoveryUiState) -> bool {
        let order = self.live_item_ids();
        let mode = match state.mode {
            InputRecoveryMode::Board => InteractionMode::Board,
            InputRecoveryMode::Compose => InteractionMode::Compose,
            InputRecoveryMode::Edit { thought_id }
                if self
                    .state
                    .board
                    .thought(thought_id)
                    .is_some_and(crate::domain::Thought::is_live) =>
            {
                InteractionMode::Edit { thought_id }
            }
            InputRecoveryMode::Edit { .. } => return false,
        };
        if state
            .focused_item
            .is_some_and(|item_id| !order.contains(&item_id))
            || state.insertion_index > order.len()
            || state.expanded_folds.iter().any(|(thought_id, index)| {
                self.state
                    .board
                    .thought(*thought_id)
                    .is_none_or(|thought| *index >= thought.annotations.len())
            })
            || !self
                .selection
                .restore_recovery_state(state.selection, &order)
        {
            return false;
        }
        let Some(board_viewport) = decode_viewport(state.board_viewport) else {
            return false;
        };
        self.state.mode = mode;
        self.state.focused_item = state.focused_item;
        self.state.insertion_index = state.insertion_index;
        self.insertion_focus = if state.insertion_focused {
            InsertionFocus::Active
        } else {
            InsertionFocus::Inactive
        };
        self.compose_presentation = if state.compose_editor_visible {
            ComposePresentation::Editor
        } else {
            ComposePresentation::Prompt
        };
        self.expanded_folds = state.expanded_folds.into_iter().collect();
        self.board_viewport = board_viewport;
        self.pending_recovery_editor = state.editor;
        self.sync_editor_from_state();
        self.layout = None;
        self.frame_presentation = None;
        self.scroll_geometry = None;
        true
    }

    pub(super) fn apply_pending_recovery_editor(&mut self) {
        let Some(state) = self.pending_recovery_editor.take() else {
            return;
        };
        let Some((_, editor)) = self.editor.as_mut() else {
            return;
        };
        editor.restore_recovery_view(
            state.cursor,
            state.selection_anchor,
            if state.previous_row_affinity {
                VisualCursorAffinity::PreviousRow
            } else {
                VisualCursorAffinity::NextRow
            },
            state.scroll_row,
        );
    }
}

fn encode_viewport(viewport: BoardViewport) -> InputRecoveryBoardViewport {
    let follows_focus = matches!(viewport, BoardViewport::FollowFocus(_));
    InputRecoveryBoardViewport {
        follows_focus,
        anchor: encode_anchor(viewport.anchor()),
    }
}

fn encode_anchor(anchor: ScrollAnchor) -> InputRecoveryScrollAnchor {
    match anchor {
        ScrollAnchor::Start => InputRecoveryScrollAnchor::Start,
        ScrollAnchor::GapBefore { thought_id, row } => {
            InputRecoveryScrollAnchor::GapBefore { thought_id, row }
        }
        ScrollAnchor::Content {
            thought_id,
            position: crate::ui::layout::scroll::ContentAnchor::Canonical(canonical_byte),
        } => InputRecoveryScrollAnchor::Content {
            thought_id,
            canonical_byte,
            annotation_index: None,
            projection_row: None,
        },
        ScrollAnchor::Content {
            thought_id,
            position:
                crate::ui::layout::scroll::ContentAnchor::Projection {
                    annotation_index,
                    row,
                    canonical_byte,
                },
        } => InputRecoveryScrollAnchor::Content {
            thought_id,
            canonical_byte,
            annotation_index: Some(annotation_index),
            projection_row: Some(row),
        },
        ScrollAnchor::Overflow(thought_id) => InputRecoveryScrollAnchor::Overflow { thought_id },
        ScrollAnchor::Separator(separator_id) => {
            InputRecoveryScrollAnchor::Separator { separator_id }
        }
        ScrollAnchor::Compose { byte } => InputRecoveryScrollAnchor::Compose { byte },
        ScrollAnchor::InsertGap => InputRecoveryScrollAnchor::InsertGap,
        ScrollAnchor::Insert => InputRecoveryScrollAnchor::Insert,
    }
}

fn decode_viewport(viewport: InputRecoveryBoardViewport) -> Option<BoardViewport> {
    let anchor = decode_anchor(viewport.anchor)?;
    Some(if viewport.follows_focus {
        BoardViewport::FollowFocus(anchor)
    } else {
        BoardViewport::Manual(anchor)
    })
}

fn decode_anchor(anchor: InputRecoveryScrollAnchor) -> Option<ScrollAnchor> {
    Some(match anchor {
        InputRecoveryScrollAnchor::Start => ScrollAnchor::Start,
        InputRecoveryScrollAnchor::GapBefore { thought_id, row } => {
            ScrollAnchor::GapBefore { thought_id, row }
        }
        InputRecoveryScrollAnchor::Content {
            thought_id,
            canonical_byte,
            annotation_index: None,
            projection_row: None,
        } => ScrollAnchor::Content {
            thought_id,
            position: crate::ui::layout::scroll::ContentAnchor::Canonical(canonical_byte),
        },
        InputRecoveryScrollAnchor::Content {
            thought_id,
            canonical_byte,
            annotation_index: Some(annotation_index),
            projection_row: Some(row),
        } => ScrollAnchor::Content {
            thought_id,
            position: crate::ui::layout::scroll::ContentAnchor::Projection {
                annotation_index,
                row,
                canonical_byte,
            },
        },
        InputRecoveryScrollAnchor::Content { .. } => return None,
        InputRecoveryScrollAnchor::Overflow { thought_id } => ScrollAnchor::Overflow(thought_id),
        InputRecoveryScrollAnchor::Separator { separator_id } => {
            ScrollAnchor::Separator(separator_id)
        }
        InputRecoveryScrollAnchor::Compose { byte } => ScrollAnchor::Compose { byte },
        InputRecoveryScrollAnchor::InsertGap => ScrollAnchor::InsertGap,
        InputRecoveryScrollAnchor::Insert => ScrollAnchor::Insert,
    })
}

#[cfg(test)]
#[path = "continuity/tests.rs"]
mod tests;
