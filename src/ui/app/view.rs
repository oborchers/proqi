//! Read-only UI projections and frame preparation.

use super::{BoardApp, invocation::InvocationChoiceView, palette, search, transfer};
use crate::{
    application::InteractionMode,
    domain::ThoughtId,
    ports::{
        agent::AgentTarget,
        editor::{EditCommand, EditorSnapshot},
    },
    ui::{HitTarget, KeyBindings},
};
use std::borrow::Cow;

impl BoardApp {
    pub(super) fn current_content(&self, thought_id: ThoughtId) -> Option<String> {
        self.pending_edit
            .as_ref()
            .filter(|pending| pending.thought_id == thought_id)
            .map(|pending| pending.after.content.clone())
            .or_else(|| {
                self.state
                    .board
                    .thought(thought_id)
                    .map(|thought| thought.content.clone())
            })
    }

    /// Return the active editor snapshot, if edit mode is active.
    #[must_use]
    pub fn editor_snapshot(&self) -> Option<EditorSnapshot> {
        self.editor.as_ref().map(|(_, editor)| editor.snapshot())
    }

    pub(in crate::ui) fn editor_presentation(
        &self,
    ) -> Option<&crate::ui::projection::EditorPresentation> {
        self.layout.as_ref()?;
        self.frame_presentation
            .as_ref()
            .and_then(crate::ui::projection::FramePresentation::editor)
    }

    pub(super) fn active_presented_thought(
        &self,
    ) -> Option<Cow<'_, crate::ui::projection::PresentedThought>> {
        let thought_id = self.active_thought_id()?;
        let content = self.current_content(thought_id)?;
        if let Some(thought) = self
            .frame_presentation
            .as_ref()
            .and_then(|presentation| presentation.thought(thought_id))
            .filter(|thought| {
                thought.canonical_content == content
                    && thought
                        .presentation
                        .substitutions
                        .iter()
                        .all(|substitution| {
                            substitution.collapsed
                                != self
                                    .expanded_folds
                                    .contains(&(thought_id, substitution.annotation_index))
                        })
            })
        {
            return Some(Cow::Borrowed(thought));
        }
        self.build_frame_presentation()
            .thought(thought_id)
            .cloned()
            .map(Cow::Owned)
    }

    pub(super) fn build_frame_presentation(&self) -> crate::ui::projection::FramePresentation {
        let thoughts = self
            .state
            .board
            .live_thoughts()
            .into_iter()
            .map(|thought| {
                let content = self
                    .current_content(thought.id)
                    .unwrap_or_else(|| thought.content.clone());
                let annotations = self.current_annotations(thought.id);
                let projection = crate::ui::annotations::project_with_health(
                    &content,
                    &annotations,
                    &self.expanded_fold_indices(thought.id),
                    |annotation_index| self.attachment_inaccessible(thought.id, annotation_index),
                )
                .unwrap_or_else(|_| {
                    crate::ui::annotations::Presentation::canonical(content.clone())
                });
                crate::ui::projection::PresentedThought {
                    thought_id: thought.id,
                    canonical_content: content,
                    presentation: projection,
                    preference: thought.presentation,
                }
            })
            .collect();
        crate::ui::projection::FramePresentation::new(thoughts)
    }

    pub(in crate::ui) fn presentation_for_layout<'a>(
        &'a self,
        layout: &crate::ui::LayoutSnapshot,
    ) -> Cow<'a, crate::ui::projection::FramePresentation> {
        if self.layout.as_ref() == Some(layout)
            && let Some(presentation) = &self.frame_presentation
        {
            return Cow::Borrowed(presentation);
        }
        let mut presentation = self.build_frame_presentation();
        self.attach_editor_presentation(&mut presentation);
        Cow::Owned(presentation)
    }

    pub(super) fn attach_editor_presentation(
        &self,
        frame: &mut crate::ui::projection::FramePresentation,
    ) {
        if self.compose_prompt_visible() {
            return;
        }
        let Some(snapshot) = self.editor_snapshot() else {
            return;
        };
        let source = match self.editor.as_ref().map(|editor| editor.0) {
            Some(super::EditorOwner::Compose) => {
                crate::ui::annotations::Presentation::canonical(snapshot.content.clone())
            }
            Some(super::EditorOwner::Thought(thought_id)) => {
                let Some(thought) = frame.thought(thought_id) else {
                    return;
                };
                thought.presentation.clone()
            }
            None => return,
        };
        frame.set_editor(&snapshot, source);
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

    pub(super) fn edit_content_mutation_blocked(&self, thought_id: ThoughtId) -> bool {
        self.submission_locked(thought_id) || self.state.deferred_board_operation_pending()
    }

    pub(super) fn edit_command_blocked(&mut self, command: &EditCommand) -> bool {
        if matches!(
            command,
            EditCommand::Move { .. }
                | EditCommand::SelectAll
                | EditCommand::ClearSelection
                | EditCommand::SetCursor { .. }
                | EditCommand::PointerStart { .. }
                | EditCommand::PointerDrag { .. }
                | EditCommand::PointerEnd
        ) {
            return false;
        }
        let blocked = matches!(self.editor.as_ref(), Some((super::EditorOwner::Thought(id), _)) if self.edit_content_mutation_blocked(*id));
        if blocked {
            self.set_warning("thought has a submission in progress");
        }
        blocked
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

    /// Whether empty Compose uses the passive prompt projection.
    #[must_use]
    pub fn compose_prompt_visible(&self) -> bool {
        self.compose_active()
            && matches!(
                self.compose_presentation,
                super::ComposePresentation::Prompt
            )
    }

    /// Whether empty Compose exposes its ordinary editor projection.
    #[must_use]
    pub fn compose_editor_visible(&self) -> bool {
        self.compose_active()
            && matches!(
                self.compose_presentation,
                super::ComposePresentation::Editor
            )
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

    pub(in crate::ui) fn picker_overflow(&self, visible: usize) -> (bool, bool) {
        if self.search.is_some() {
            return self.search_overflow(visible);
        }
        if self.transfer.is_some() {
            return self.transfer_overflow(visible);
        }
        if self.invocation_popup.is_some() {
            return self.invocation_overflow(visible);
        }
        self.palette
            .as_ref()
            .map_or((false, false), |palette| palette.overflow(visible))
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

    pub(super) fn expanded_fold_indices(&self, thought_id: ThoughtId) -> Vec<usize> {
        self.expanded_folds
            .iter()
            .filter_map(|(id, index)| (*id == thought_id).then_some(*index))
            .collect()
    }
}
