//! Read-only UI projections and frame preparation.

use ratatui_core::layout::Rect;

use crate::{
    application::{DurabilityState, InteractionMode},
    domain::ThoughtId,
    ports::{agent::AgentTarget, editor::EditorSnapshot, editor::TextViewport},
    ui::{HitTarget, KeyBindings, LayoutSnapshot, compute_layout},
};

use super::{BoardApp, palette};

impl BoardApp {
    /// Return the active editor snapshot, if edit mode is active.
    #[must_use]
    pub fn editor_snapshot(&self) -> Option<EditorSnapshot> {
        self.editor.as_ref().map(|(_, editor)| editor.snapshot())
    }

    pub(in crate::ui) fn editor_presentation(
        &self,
    ) -> Option<crate::ui::projection::EditorPresentation> {
        let thought_id = self.active_thought_id()?;
        let snapshot = self.editor_snapshot()?;
        let annotations = self.current_annotations(thought_id);
        Some(crate::ui::projection::editor_presentation(
            &snapshot,
            &annotations,
            &self.expanded_fold_indices(thought_id),
        ))
    }

    pub(in crate::ui) fn presentation_for_render(
        &self,
        thought_id: ThoughtId,
    ) -> Option<crate::ui::annotations::Presentation> {
        let content = self.content_for_render(thought_id)?;
        let annotations = self.current_annotations(thought_id);
        Some(crate::ui::annotations::project(
            &content,
            &annotations,
            &self.expanded_fold_indices(thought_id),
        ))
    }

    /// Effective interaction mode, including an ephemeral draft editor.
    #[must_use]
    pub fn interaction_mode(&self) -> InteractionMode {
        self.draft
            .as_ref()
            .map_or(self.state.mode, |draft| InteractionMode::Edit {
                thought_id: draft.thought_id,
            })
    }

    /// Focused durable thought or ephemeral draft identity.
    #[must_use]
    pub fn active_thought_id(&self) -> Option<ThoughtId> {
        self.draft
            .as_ref()
            .map(|draft| draft.thought_id)
            .or(self.state.focused_thought)
    }

    /// Number of currently visible durable thoughts and drafts.
    #[must_use]
    pub fn visible_thought_count(&self) -> usize {
        self.state.board.live_thoughts().len() + usize::from(self.draft.is_some())
    }

    /// Whether an empty, non-durable thought is currently being edited.
    #[must_use]
    pub const fn has_draft(&self) -> bool {
        self.draft.is_some()
    }

    /// Monotonic counter used by the runtime to detect new unflushed editor work.
    #[must_use]
    pub const fn edit_generation(&self) -> u64 {
        self.edit_generation
    }

    /// Whether editor content is waiting for its semantic durability boundary.
    #[must_use]
    pub const fn has_pending_edit(&self) -> bool {
        self.pending_edit.is_some()
    }

    /// Current hover target resolved from the latest rendered layout.
    #[must_use]
    pub const fn hovered(&self) -> Option<HitTarget> {
        self.hovered
    }

    /// Thought currently being dragged, when pointer reordering is active.
    #[must_use]
    pub const fn dragged_thought(&self) -> Option<ThoughtId> {
        self.dragged_thought
    }

    /// Board position currently previewed as the drag destination.
    #[must_use]
    pub const fn drag_target(&self) -> Option<usize> {
        self.drag_target
    }

    /// Filtered command labels and current selection for rendering.
    #[must_use]
    pub fn palette_view(&self) -> Option<(String, Vec<String>, usize)> {
        self.palette.as_ref().map(palette::PaletteState::view)
    }

    /// Active board bindings used by hints and command translation.
    #[must_use]
    pub const fn keybindings(&self) -> &KeyBindings {
        &self.settings.keybindings
    }

    /// Currently verified submission targets, empty when the enhancement is unavailable.
    #[must_use]
    pub fn agent_targets(&self) -> &[AgentTarget] {
        &self.agent_targets
    }

    /// Whether directional submission targeting is active and removes on acceptance.
    #[must_use]
    pub fn submission_mode(&self) -> Option<bool> {
        self.submission_mode.map(|mode| mode.remove)
    }

    /// Prepare current frame geometry without changing the logical cursor.
    pub fn prepare_layout(&mut self, viewport: TextViewport) {
        self.viewport = viewport;
        if let Some((_, editor)) = &mut self.editor {
            editor.set_viewport(viewport);
        }
    }

    /// Recompute one authoritative frame layout and reflow the active editor.
    pub fn prepare_frame(&mut self, area: Rect) -> LayoutSnapshot {
        let mut layout_state = self.presentation_state();
        if self.manual_board_scroll {
            layout_state.focused_thought = None;
        }
        let has_status = self.status.is_some()
            || matches!(self.state.durability, DurabilityState::Failed { .. });
        let first_editor = self.editor_presentation();
        let first = compute_layout(
            &layout_state,
            first_editor.as_ref().map(|view| &view.snapshot),
            area,
            self.first_visible,
            &self.expanded,
            has_status,
            !self.agent_targets.is_empty(),
        );
        let height = self.focused_height(&first);
        self.prepare_layout(TextViewport::new(first.content_width, height));
        let editor = self.editor_presentation();
        let mut layout = compute_layout(
            &layout_state,
            editor.as_ref().map(|view| &view.snapshot),
            area,
            first.first_index,
            &self.expanded,
            has_status,
            !self.agent_targets.is_empty(),
        );
        self.configure_overlay(&mut layout);
        layout.configure_agent_controls(&self.agent_targets, self.submission_mode());
        let final_height = self.focused_height(&layout);
        self.prepare_layout(TextViewport::new(layout.content_width, final_height));
        self.first_visible = layout.first_index;
        self.layout = Some(layout.clone());
        layout
    }

    fn focused_height(&self, layout: &LayoutSnapshot) -> u16 {
        self.active_thought_id()
            .and_then(|id| layout.thought(id))
            .map_or(layout.board.height.max(1), |thought| {
                thought.text_area.height.max(1)
            })
    }

    fn configure_overlay(&self, layout: &mut LayoutSnapshot) {
        let palette_items = self
            .palette
            .as_ref()
            .map_or(0, palette::PaletteState::match_count);
        let search_items = self.search_match_count();
        let preferred_rows = if self.help {
            6
        } else if self.palette.is_some() {
            palette_items.max(2)
        } else if self.search.is_some() {
            search_items.max(2)
        } else {
            0
        };
        layout.configure_overlay(palette_items.max(search_items), preferred_rows);
    }

    fn presentation_state(&self) -> crate::application::AppState {
        let mut state = self.layout_state_with_draft();
        let ids = state
            .board
            .thoughts()
            .iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(thought) = state.board.thought_mut(id) {
                thought.content = crate::ui::annotations::project(
                    &thought.content,
                    &thought.annotations,
                    &self.expanded_fold_indices(id),
                )
                .content;
                thought.annotations.clear();
            }
        }
        state
    }

    fn expanded_fold_indices(&self, thought_id: ThoughtId) -> Vec<usize> {
        self.expanded_folds
            .iter()
            .filter_map(|(id, index)| (*id == thought_id).then_some(*index))
            .collect()
    }
}
