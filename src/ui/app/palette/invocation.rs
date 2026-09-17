//! Captured Commands context resolves labels, applicability, and relevance once.

mod presentation;
mod recovery;
mod selection;

use crate::{
    application::InteractionMode,
    domain::{ContentAnnotationKind, Thought},
    ui::{
        CommandApplicability, CommandLabel, CommandMetadata, CommandRelevance,
        app::screenshot::ScreenshotPaletteAction,
    },
};

use super::{
    super::{BoardApp, palette_handoff::EditorSelectionHandoff},
    PaletteHistoryContext,
};
use recovery::RecoveryContext;
use selection::selection_is_contiguous;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Applicability {
    pub(super) enabled: bool,
    pub(super) reason: Option<&'static str>,
}

impl Applicability {
    const ENABLED: Self = Self {
        enabled: true,
        reason: None,
    };

    const fn disabled(reason: &'static str) -> Self {
        Self {
            enabled: false,
            reason: Some(reason),
        }
    }
}

pub(super) struct CommandContext {
    board: BoardContext,
    mutation: MutationContext,
    features: FeatureContext,
    recovery: RecoveryContext,
    attachments: AttachmentContext,
    screenshot: ScreenshotContext,
    selection: SelectionContext,
}

struct BoardContext {
    live_thought_count: usize,
    live_item_count: usize,
    focus: BoardFocus,
    mode: InteractionMode,
    operation_pending: bool,
}

struct BoardFocus {
    item: bool,
    thought: bool,
    insertion: bool,
}

struct MutationContext {
    focused_item_mutable: bool,
    focused_mutable: bool,
    all_thoughts_mutable: bool,
}

struct FeatureContext {
    submit_supported: bool,
    installed_highlights: bool,
}

struct AttachmentContext {
    present: bool,
    refreshing: bool,
}

pub(super) struct ScreenshotContext {
    action: ScreenshotPaletteAction,
    retry: bool,
}

struct SelectionContext {
    count: usize,
    thought_count: usize,
    contiguous: bool,
    editor_handoff: Option<EditorSelectionHandoff>,
    merge_handoff: Option<Vec<Thought>>,
}

impl CommandContext {
    pub(super) fn applicability(&self, metadata: CommandMetadata) -> Applicability {
        self.applicability_with_history(metadata, PaletteHistoryContext::EMPTY)
    }

    pub(super) fn applicability_with_history(
        &self,
        metadata: CommandMetadata,
        history: PaletteHistoryContext,
    ) -> Applicability {
        use CommandApplicability as A;
        match metadata.applicability {
            A::Always => Applicability::ENABLED,
            A::WritableBoard => self.when_writable_board(),
            A::HasItems => Self::when(self.board.live_item_count > 0, "Board is empty"),
            A::BoardNonempty => self.board_nonempty_applicability(),
            A::Copy => self.copy_applicability(),
            A::Cut => self.cut_applicability(),
            A::MutableItem => self.when_item_mutable(),
            A::MutableThought | A::Editor => self.when_mutable(),
            A::Reorder => self.reorder_applicability(),
            A::BoardItem => Self::when(self.board_item(), "Available from Board focus"),
            A::BoardThought => Self::when(self.board_thought(), "Available from Board focus"),
            A::Submission => self.submission_applicability(),
            A::SubmissionAll => self.all_submission_applicability(),
            A::ScreenshotRetry => self.screenshot_retry_applicability(),
            A::Split => self.when_handoff(false),
            A::Extract => self.when_handoff(true),
            A::Merge => self.merge_applicability(),
            A::ScreenshotInbox => self.screenshot_inbox_applicability(),
            A::RetryStorage => self.recovery.retry_applicability(),
            A::ExportRecovery => self.recovery.export_applicability(),
            A::Quit => self.recovery.quit_applicability(),
            A::QueryUndo => Self::when(history.can_undo, "Nothing to undo in the query"),
            A::QueryRedo => Self::when(history.can_redo, "Nothing to redo in the query"),
            A::Attachments => self.attachments_applicability(),
            A::InstalledHighlights => Self::when(
                self.features.installed_highlights,
                "Unavailable for this Proqi installation",
            ),
        }
    }

    pub(super) fn relevance(
        &self,
        metadata: CommandMetadata,
        history: PaletteHistoryContext,
    ) -> Option<u8> {
        if !self.applicability_with_history(metadata, history).enabled {
            return None;
        }
        match metadata.relevance {
            CommandRelevance::Always(priority) => Some(priority),
            CommandRelevance::FocusedThought(priority) if self.board.focus.thought => {
                Some(priority)
            }
            CommandRelevance::Selection(priority) if self.selection.count >= 2 => Some(priority),
            CommandRelevance::Editor(priority) if self.selection.editor_handoff.is_some() => {
                Some(priority)
            }
            CommandRelevance::Submission(priority) if self.features.submit_supported => {
                Some(priority)
            }
            CommandRelevance::QueryUndo(priority) if history.can_undo => Some(priority),
            CommandRelevance::QueryRedo(priority) if history.can_redo => Some(priority),
            CommandRelevance::StorageRecovery(priority) if self.recovery.failed() => Some(priority),
            CommandRelevance::ScreenshotActive(priority)
                if matches!(
                    self.screenshot.action,
                    ScreenshotPaletteAction::Disable | ScreenshotPaletteAction::Resume
                ) =>
            {
                Some(priority)
            }
            CommandRelevance::ScreenshotRetry(priority) if self.screenshot.retry => Some(priority),
            CommandRelevance::Never
            | CommandRelevance::FocusedThought(_)
            | CommandRelevance::Selection(_)
            | CommandRelevance::Editor(_)
            | CommandRelevance::Submission(_)
            | CommandRelevance::QueryUndo(_)
            | CommandRelevance::QueryRedo(_)
            | CommandRelevance::StorageRecovery(_)
            | CommandRelevance::ScreenshotActive(_)
            | CommandRelevance::ScreenshotRetry(_) => None,
        }
    }

    pub(super) const fn command_label(&self, label: CommandLabel) -> &'static str {
        match label {
            CommandLabel::Static(label) => label,
            CommandLabel::ScreenshotInbox {
                enable,
                disable,
                resume,
                unavailable,
            } => match self.screenshot.action {
                ScreenshotPaletteAction::Enable => enable,
                ScreenshotPaletteAction::Disable => disable,
                ScreenshotPaletteAction::Resume => resume,
                ScreenshotPaletteAction::Unavailable => unavailable,
            },
        }
    }

    pub(super) fn set_screenshot(&mut self, screenshot: ScreenshotContext) {
        self.screenshot = screenshot;
    }

    pub(super) fn set_submit_supported(&mut self, supported: bool) {
        self.features.submit_supported = supported;
    }

    pub(super) fn set_recovery(&mut self, recovery: RecoveryContext) {
        self.recovery = recovery;
    }

    pub(super) fn set_attachments_refreshing(&mut self, refreshing: bool) {
        self.attachments.refreshing = refreshing;
    }

    pub(super) fn take_selection_handoff(&mut self) -> Option<EditorSelectionHandoff> {
        self.selection.editor_handoff.take()
    }

    pub(super) fn take_merge_handoff(&mut self) -> Option<Vec<Thought>> {
        self.selection.merge_handoff.take()
    }

    const fn when(condition: bool, reason: &'static str) -> Applicability {
        if condition {
            Applicability::ENABLED
        } else {
            Applicability::disabled(reason)
        }
    }

    const fn when_mutable(&self) -> Applicability {
        if self.selection.thought_count == 0 {
            Applicability::disabled("No thought is focused")
        } else if self.recovery.failed() {
            Applicability::disabled("Resolve the failed save first")
        } else if !self.mutation.focused_mutable {
            Applicability::disabled("Thought has an operation in progress")
        } else {
            Applicability::ENABLED
        }
    }

    const fn when_item_mutable(&self) -> Applicability {
        if !self.board.focus.item {
            Applicability::disabled("No Board item is focused")
        } else if self.recovery.failed() {
            Applicability::disabled("Resolve the failed save first")
        } else if !self.mutation.focused_item_mutable {
            Applicability::disabled("Board item has an operation in progress")
        } else {
            Applicability::ENABLED
        }
    }

    const fn when_writable_board(&self) -> Applicability {
        if self.recovery.failed() {
            Applicability::disabled("Resolve the failed save first")
        } else if self.board.operation_pending {
            Applicability::disabled("A board operation is still being saved")
        } else {
            Applicability::ENABLED
        }
    }

    fn submission_applicability(&self) -> Applicability {
        let mutable = self.when_mutable();
        if !mutable.enabled {
            return mutable;
        }
        Self::when(
            self.features.submit_supported,
            "No verified agent is available",
        )
    }

    const fn screenshot_retry_applicability(&self) -> Applicability {
        if self.recovery.failed() {
            Applicability::disabled("Resolve the failed save first")
        } else {
            Self::when(self.screenshot.retry, "No failed capture to retry")
        }
    }

    const fn screenshot_inbox_applicability(&self) -> Applicability {
        if matches!(self.screenshot.action, ScreenshotPaletteAction::Unavailable) {
            Applicability::disabled("Screenshot Inbox is stopping")
        } else if self.recovery.failed()
            && matches!(
                self.screenshot.action,
                ScreenshotPaletteAction::Enable | ScreenshotPaletteAction::Resume
            )
        {
            Applicability::disabled("Resolve the failed save first")
        } else {
            Applicability::ENABLED
        }
    }

    fn all_submission_applicability(&self) -> Applicability {
        if !self.has_live_thoughts() {
            return Applicability::disabled("Board has no thoughts");
        }
        if self.recovery.failed() {
            return Applicability::disabled("Resolve the failed save first");
        }
        if !self.mutation.all_thoughts_mutable {
            return Applicability::disabled("A thought has an operation in progress");
        }
        Self::when(
            self.features.submit_supported,
            "No verified agent is available",
        )
    }

    fn reorder_applicability(&self) -> Applicability {
        let mutable = self.when_item_mutable();
        if !mutable.enabled {
            return mutable;
        }
        if self.selection.count > 1 {
            return Applicability::disabled("Unavailable for multiple selected items");
        }
        Self::when(self.board.live_item_count > 1, "Nothing to reorder")
    }

    const fn board_nonempty_applicability(&self) -> Applicability {
        if self.board.live_item_count == 0 {
            Applicability::disabled("Board is empty")
        } else if !matches!(self.board.mode, InteractionMode::Board) {
            Applicability::disabled("Available from Board focus")
        } else {
            Applicability::ENABLED
        }
    }

    const fn attachments_applicability(&self) -> Applicability {
        if !self.attachments.present {
            Applicability::disabled("No attachments to refresh")
        } else if self.attachments.refreshing {
            Applicability::disabled("Attachment refresh is in progress")
        } else {
            Applicability::ENABLED
        }
    }

    fn when_handoff(&self, needs_selection: bool) -> Applicability {
        let mutable = self.when_mutable();
        if !mutable.enabled {
            return mutable;
        }
        let Some(handoff) = &self.selection.editor_handoff else {
            return Applicability::disabled("Open Commands from the editor");
        };
        Self::when(
            !needs_selection || handoff.has_selection(),
            "Select text in the editor first",
        )
    }

    fn merge_applicability(&self) -> Applicability {
        if self.selection.thought_count < 2 {
            return Applicability::disabled("Select at least two thoughts");
        }
        if !self.selection.contiguous {
            return Applicability::disabled("Selection must be contiguous");
        }
        self.when_mutable()
    }

    const fn has_live_thoughts(&self) -> bool {
        self.board.live_thought_count > 0
    }

    const fn board_thought(&self) -> bool {
        matches!(self.board.mode, InteractionMode::Board)
            && !self.board.focus.insertion
            && self.board.focus.thought
    }

    const fn board_item(&self) -> bool {
        matches!(self.board.mode, InteractionMode::Board)
            && !self.board.focus.insertion
            && self.board.focus.item
    }
}

impl BoardApp {
    pub(super) fn capture_recovery_command_context(&self) -> RecoveryContext {
        RecoveryContext::capture(&self.state.durability, self.recovery_exported_for)
    }

    pub(super) fn board_thought_command_available(&self) -> bool {
        matches!(self.state.mode, InteractionMode::Board)
            && !self.insertion_focused()
            && self.state.focused_thought_id().is_some()
    }

    pub(super) fn capture_screenshot_command_context(&self) -> ScreenshotContext {
        ScreenshotContext {
            action: self.screenshot_palette_action(),
            retry: self.screenshot_retry_ready(),
        }
    }

    pub(super) fn capture_command_context(&mut self) -> CommandContext {
        let selected_ids = self.action_thought_ids();
        let selected_thought_count = selected_ids.len();
        let has_thought = self.active_thought_id().is_some();
        let has_item = self.command_has_item(has_thought);
        let has_action_thought = selected_thought_count > 0;
        let mutable_thought = has_action_thought
            && !self.state.deferred_board_operation_pending()
            && selected_ids.iter().all(|id| {
                !self.submission_locked(*id)
                    && !self
                        .pending_transfer_removals
                        .values()
                        .any(|pending| pending == id)
            });
        let item_mutable = if has_action_thought {
            mutable_thought
        } else {
            has_item && !self.state.deferred_board_operation_pending()
        };
        let live_thoughts = self.state.board.live_thoughts();
        let selection_count = self.selection_len();
        let selection_contiguous =
            selection_is_contiguous(self, &selected_ids, selected_thought_count);
        let merge_handoff = (selected_thought_count >= 2).then(|| {
            selected_ids
                .iter()
                .filter_map(|id| self.state.board.thought(*id).cloned())
                .collect()
        });
        let has_attachments = self.state.board.live_thoughts().iter().any(|thought| {
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
            mutation: MutationContext {
                focused_item_mutable: item_mutable,
                focused_mutable: mutable_thought,
                all_thoughts_mutable: live_thoughts.iter().all(|thought| {
                    !self.submission_locked(thought.id)
                        && !self
                            .pending_transfer_removals
                            .values()
                            .any(|pending| *pending == thought.id)
                }),
            },
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

#[cfg(test)]
mod tests;
