//! Read-only UI projections and frame preparation.

use crate::{
    application::{DurabilityState, InteractionMode},
    domain::ThoughtId,
    ports::{agent::AgentTarget, editor::EditorSnapshot, editor::TextViewport},
    ui::{HitTarget, KeyBindings, LayoutSnapshot},
};
use ratatui_core::layout::Rect;
use unicode_width::UnicodeWidthStr as _;

use super::{BoardApp, invocation::InvocationChoiceView, palette, search, transfer};

impl BoardApp {
    /// Return the active editor snapshot, if edit mode is active.
    #[must_use]
    pub fn editor_snapshot(&self) -> Option<EditorSnapshot> {
        self.editor.as_ref().map(|(_, editor)| editor.snapshot())
    }

    pub(in crate::ui) fn editor_presentation(
        &self,
    ) -> Option<crate::ui::projection::EditorPresentation> {
        let snapshot = self.editor_snapshot()?;
        let (annotations, expanded) = match self.editor.as_ref()?.0 {
            super::EditorOwner::Compose => (Vec::new(), Vec::new()),
            super::EditorOwner::Thought(thought_id) => (
                self.current_annotations(thought_id),
                self.expanded_fold_indices(thought_id),
            ),
        };
        Some(crate::ui::projection::editor_presentation(
            &snapshot,
            &annotations,
            &expanded,
        ))
    }

    pub(in crate::ui) fn presentation_for_render(
        &self,
        thought_id: ThoughtId,
    ) -> Option<crate::ui::annotations::Presentation> {
        let content = self
            .state
            .board
            .thought(thought_id)
            .map(|thought| thought.content.clone())?;
        let annotations = self.current_annotations(thought_id);
        Some(crate::ui::annotations::project(
            &content,
            &annotations,
            &self.expanded_fold_indices(thought_id),
        ))
    }

    /// Effective interaction mode.
    #[must_use]
    pub fn interaction_mode(&self) -> InteractionMode {
        self.state.mode
    }

    /// Focused durable thought identity.
    #[must_use]
    pub fn active_thought_id(&self) -> Option<ThoughtId> {
        if self.insertion_focused() || matches!(self.state.mode, InteractionMode::Compose) {
            return None;
        }
        self.state.focused_thought
    }

    /// Whether a thought belongs to the explicit board multi-selection.
    #[must_use]
    pub fn thought_selected(&self, thought_id: ThoughtId) -> bool {
        self.selection.contains(thought_id)
    }

    /// Ordered thoughts addressed by the next board action.
    pub(super) fn action_thought_ids(&self) -> Vec<ThoughtId> {
        let order = self
            .state
            .board
            .live_thoughts()
            .into_iter()
            .map(|thought| thought.id)
            .collect::<Vec<_>>();
        let selected = self.selection.selected_in(&order);
        if selected.is_empty() {
            self.state.focused_thought.into_iter().collect()
        } else {
            selected
        }
    }

    pub(super) fn submission_locked(&self, thought_id: ThoughtId) -> bool {
        self.state.thought_locked(thought_id)
    }

    /// Number of currently visible durable thoughts.
    #[must_use]
    pub fn visible_thought_count(&self) -> usize {
        self.state.board.live_thoughts().len()
    }

    /// Whether the board insertion row owns keyboard focus.
    #[must_use]
    pub fn insertion_focused(&self) -> bool {
        matches!(self.state.mode, InteractionMode::Board)
            && (matches!(self.insertion_focus, super::InsertionFocus::Active)
                || self.state.board.live_thoughts().is_empty())
    }

    /// Whether the transient insertion editor owns input and cursor focus.
    #[must_use]
    pub fn compose_active(&self) -> bool {
        matches!(self.state.mode, InteractionMode::Compose)
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
    pub fn hovered(&self) -> Option<HitTarget> {
        if self.selection_is_empty() {
            self.hovered
        } else {
            None
        }
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

    /// Filtered discovered invocations and current selection for rendering.
    #[must_use]
    pub(in crate::ui) fn discovered_invocation_view(
        &self,
    ) -> Option<(String, Vec<InvocationChoiceView>, usize)> {
        self.invocation_view()
    }

    /// Current session-name input when the rename prompt is active.
    #[must_use]
    pub fn session_rename_view(&self) -> Option<&str> {
        self.rename.as_deref()
    }

    /// Searchable destination sessions for explicit cross-session delivery.
    #[must_use]
    pub fn session_transfer_view(&self) -> Option<(String, Vec<String>, usize)> {
        self.transfer_view()
    }

    /// UTF-8 byte cursor for the active searchable overlay query.
    #[must_use]
    pub fn overlay_query_cursor(&self) -> Option<usize> {
        self.search
            .as_ref()
            .map(search::SearchState::query_cursor)
            .or_else(|| {
                self.transfer
                    .as_ref()
                    .map(transfer::TransferState::query_cursor)
            })
            .or_else(|| self.invocation_query_cursor())
            .or_else(|| {
                self.palette
                    .as_ref()
                    .map(palette::PaletteState::query_cursor)
            })
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

    /// Whether at least one verified target supports immediate submission.
    #[must_use]
    pub fn supports_submission(&self) -> bool {
        self.agent_targets
            .iter()
            .any(|target| target.delivery.supports())
    }

    /// Active delivery intention while a direction is being selected.
    #[must_use]
    pub fn submission_mode(&self) -> Option<crate::ports::agent::SubmissionDisposition> {
        self.submission_mode.as_ref().map(|mode| mode.disposition)
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
        self.reset_overlay_activation_for_geometry(area);
        let layout_state = self.presentation_state();
        let first_editor = self.editor_presentation();
        let has_status = self.status_view().is_some()
            || matches!(self.state.durability, DurabilityState::Failed { .. });
        let (first, first_scroll) = crate::ui::layout::compute_for_app(
            &layout_state,
            first_editor.as_ref().map(|view| &view.snapshot),
            area,
            self.insertion_focused(),
            !self.agent_targets.is_empty(),
            has_status,
            self.settings.density,
            &self.settings.keybindings,
            self.board_viewport,
        );
        let height = self.focused_height(&first);
        self.prepare_layout(TextViewport::new(first.content_width, height));
        let editor = self.editor_presentation();
        let viewport = self.board_viewport.at(first_scroll.current);
        let (mut layout, scroll) = crate::ui::layout::compute_for_app(
            &layout_state,
            editor.as_ref().map(|view| &view.snapshot),
            area,
            self.insertion_focused(),
            !self.agent_targets.is_empty(),
            has_status,
            self.settings.density,
            &self.settings.keybindings,
            viewport,
        );
        self.configure_overlay(&mut layout);
        layout.configure_agent_controls_with_keys(
            &self.agent_targets,
            self.submission_mode(),
            self.interaction_mode(),
            &self.settings.keybindings,
        );
        let summary = self.footer_summary(layout.footer_context.width.saturating_sub(4));
        let session_id = self
            .settings
            .show_session_id
            .then(|| self.state.board.session.id.to_string());
        layout.configure_footer_summary(
            summary,
            self.session_display_name().to_owned(),
            session_id,
        );
        let final_height = self.focused_height(&layout);
        self.prepare_layout(TextViewport::new(layout.content_width, final_height));
        self.board_viewport = self.board_viewport.at(scroll.current);
        self.scroll_geometry = Some(scroll);
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
        let screenshot_items = usize::from(self.screenshot.takeover.is_some()) * 2;
        let update_items = usize::from(self.update_prompt.is_some()) * 3;
        let palette_items = self
            .palette
            .as_ref()
            .map_or(0, palette::PaletteState::match_count);
        let invocation_items = self.invocation_match_count();
        let search_items = self.search_match_count();
        let transfer_items = self.transfer_match_count();
        let preferred_rows = if self.screenshot.takeover.is_some() {
            2
        } else if self.update_prompt.is_some() {
            4
        } else if self.help {
            let content_width = layout.board.width.min(58).saturating_sub(2);
            crate::ui::shortcuts::row_count(self, content_width)
        } else if self.rename.is_some() {
            2
        } else if self.invocation_popup.is_some() {
            invocation_items.max(2)
        } else if self.palette.is_some() {
            palette_items.max(2)
        } else if self.transfer.is_some() {
            transfer_items.max(2)
        } else if self.search.is_some() {
            search_items.max(2)
        } else {
            0
        };
        layout.configure_overlay(
            screenshot_items
                .max(palette_items)
                .max(search_items)
                .max(transfer_items)
                .max(invocation_items)
                .max(update_items),
            preferred_rows,
        );
    }

    fn footer_summary(&self, available_width: u16) -> String {
        let count = self.visible_thought_count();
        let noun = if count == 1 { "thought" } else { "thoughts" };
        let mode = match self.interaction_mode() {
            InteractionMode::Board if self.range_latched() => "range",
            InteractionMode::Board => "board",
            InteractionMode::Compose => "compose",
            InteractionMode::Edit { .. } => "edit",
        };
        let durability = self.durability_summary();
        let inbox = self
            .screenshot_footer_state(false)
            .map_or_else(String::new, |label| format!(" · {label}"));
        let complete = format!("{count} {noun} · {mode} · {durability}{inbox}");
        if complete.width() <= usize::from(available_width) {
            return complete;
        }
        let compact_inbox = self
            .screenshot_footer_state(true)
            .map_or_else(String::new, |label| format!(" · {label}"));
        let compact = format!("{count} · {mode} · {durability}{compact_inbox}");
        if compact.width() <= usize::from(available_width) {
            return compact;
        }
        if let Some(paused) = self.screenshot_footer_state(true) {
            return paused;
        }
        format!("{count} {durability}")
    }

    pub(crate) fn session_display_name(&self) -> &str {
        let session = &self.state.board.session;
        session.name.as_deref().unwrap_or_else(|| {
            session
                .last_opened_cwd
                .file_name()
                .or_else(|| session.origin_cwd.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("untitled")
        })
    }

    fn durability_summary(&self) -> &'static str {
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            "unsaved"
        } else if self.has_pending_edit()
            || matches!(self.state.durability, DurabilityState::Pending { .. })
        {
            "saving"
        } else {
            "saved"
        }
    }

    fn presentation_state(&self) -> crate::application::AppState {
        let mut state = self.state.clone();
        if self.insertion_focused() {
            state.focused_thought = None;
        }
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
