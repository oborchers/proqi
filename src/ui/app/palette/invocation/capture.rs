//! Capture one stable Commands context from the current application state.

use crate::{
    application::InteractionMode,
    domain::{ContentAnnotationKind, ThoughtId},
};

use super::{
    AttachmentContext, BoardContext, BoardFocus, CommandContext, FeatureContext, MutationContext,
    ScreenshotContext, SelectionContext, recovery::RecoveryContext,
    selection::selection_is_contiguous,
};
use crate::ui::app::BoardApp;

impl BoardApp {
    pub(in crate::ui::app::palette) fn capture_recovery_command_context(&self) -> RecoveryContext {
        RecoveryContext::capture(&self.state.durability, self.recovery_exported_for)
    }

    pub(in crate::ui::app::palette) fn board_thought_command_available(&self) -> bool {
        matches!(self.state.mode, InteractionMode::Board)
            && !self.insertion_focused()
            && self.state.focused_thought_id().is_some()
    }

    pub(in crate::ui::app::palette) fn capture_screenshot_command_context(
        &self,
    ) -> ScreenshotContext {
        ScreenshotContext {
            action: self.screenshot_palette_action(),
            retry: self.screenshot_retry_ready(),
        }
    }

    pub(in crate::ui::app::palette) fn capture_command_context(&mut self) -> CommandContext {
        let selected_ids = self.action_thought_ids();
        let selected_thought_count = selected_ids.len();
        let has_thought = self.active_thought_id().is_some();
        let has_item = self.command_has_item(has_thought);
        let live_thoughts = self.state.board.live_thoughts();
        let mutation = self.capture_mutation_context(&selected_ids, has_item);
        let selection_count = self.selection_len();
        let selection_contiguous =
            selection_is_contiguous(self, &selected_ids, selected_thought_count);
        let merge_handoff = (selected_thought_count >= 2).then(|| {
            selected_ids
                .iter()
                .filter_map(|id| self.state.board.thought(*id).cloned())
                .collect()
        });
        let has_attachments = live_thoughts.iter().any(|thought| {
            thought.annotations.iter().any(|annotation| {
                matches!(annotation.kind, ContentAnnotationKind::Attachment { .. })
            })
        });
        CommandContext {
            board: BoardContext {
                live_thought_count: live_thoughts.len(),
                live_item_count: self.state.board.live_items().len(),
                focus: BoardFocus {
                    item: has_item,
                    thought: has_thought,
                    insertion: self.insertion_focused(),
                },
                mode: self.state.mode,
                operation_pending: self.state.deferred_board_operation_pending(),
            },
            mutation,
            features: FeatureContext {
                submit_supported: self.supports_submission(),
                installed_highlights: self.installed_highlights.is_some(),
            },
            recovery: self.capture_recovery_command_context(),
            attachments: AttachmentContext {
                present: has_attachments,
                refreshing: self.state.attachments.manual_refresh_active(),
            },
            screenshot: self.capture_screenshot_command_context(),
            selection: SelectionContext {
                count: selection_count,
                thought_count: selected_thought_count,
                contiguous: selection_contiguous,
                editor_handoff: self.palette_selection_handoff.take(),
                merge_handoff,
            },
        }
    }

    fn capture_mutation_context(
        &self,
        selected_ids: &[ThoughtId],
        has_item: bool,
    ) -> MutationContext {
        let action_thoughts = !selected_ids.is_empty()
            && !self.state.deferred_board_operation_pending()
            && selected_ids.iter().all(|id| self.thought_mutable(*id));
        let focused_thought = self.state.focused_thought_id().is_some_and(|id| {
            !self.state.deferred_board_operation_pending() && self.thought_mutable(id)
        });
        let focused_item = if selected_ids.is_empty() {
            has_item && !self.state.deferred_board_operation_pending()
        } else {
            action_thoughts
        };
        MutationContext {
            focused_item: focused_item.into(),
            action_thoughts: action_thoughts.into(),
            focused_thought: focused_thought.into(),
            all_thoughts: self
                .state
                .board
                .live_thoughts()
                .iter()
                .all(|thought| self.thought_mutable(thought.id))
                .into(),
        }
    }

    fn thought_mutable(&self, thought_id: ThoughtId) -> bool {
        !self.submission_locked(thought_id)
            && !self
                .pending_transfer_removals
                .values()
                .any(|pending| *pending == thought_id)
    }

    fn command_has_item(&self, has_thought: bool) -> bool {
        match self.state.mode {
            InteractionMode::Board => {
                !self.insertion_focused() && self.state.focused_item.is_some()
            }
            InteractionMode::Edit { .. } => has_thought,
            InteractionMode::Compose => false,
        }
    }
}
