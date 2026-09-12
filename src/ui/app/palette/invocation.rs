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

use super::super::{BoardApp, palette_handoff::EditorSelectionHandoff};
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
    history: HistoryContext,
    recovery: RecoveryContext,
    attachments: AttachmentContext,
    screenshot: ScreenshotContext,
    selection: SelectionContext,
}

struct BoardContext {
    live_thought_count: usize,
    focused: bool,
    mode: InteractionMode,
    insertion_focused: bool,
    operation_pending: bool,
}

struct MutationContext {
    focused_mutable: bool,
    all_thoughts_mutable: bool,
}

struct FeatureContext {
    submit_supported: bool,
    installed_highlights: bool,
}

struct HistoryContext {
    can_undo: bool,
    can_redo: bool,
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
    contiguous: bool,
    editor_handoff: Option<EditorSelectionHandoff>,
    merge_handoff: Option<Vec<Thought>>,
}

impl CommandContext {
    pub(super) fn applicability(&self, metadata: CommandMetadata) -> Applicability {
        use CommandApplicability as A;
        match metadata.applicability {
            A::Always => Applicability::ENABLED,
            A::WritableBoard => self.when_writable_board(),
            A::HasThoughts => Self::when(self.has_live_thoughts(), "Board has no thoughts"),
            A::BoardNonempty => self.board_nonempty_applicability(),
            A::Copy => self.copy_applicability(),
            A::Cut => self.cut_applicability(),
            A::MutableThought | A::Editor => self.when_mutable(),
            A::Reorder => self.reorder_applicability(),
            A::BoardThought => Self::when(self.board_thought(), "Available from Board focus"),
            A::Submission => self.submission_applicability(),
            A::SubmissionAll => self.all_submission_applicability(),
            A::ScreenshotRetry => Self::when(self.screenshot.retry, "No failed capture to retry"),
            A::Split => self.when_handoff(false),
            A::Extract => self.when_handoff(true),
            A::Merge => self.merge_applicability(),
            A::ScreenshotInbox => Self::when(
                self.screenshot.action != ScreenshotPaletteAction::Unavailable,
                "Screenshot Inbox is stopping",
            ),
            A::RetryStorage => self.recovery.retry_applicability(),
            A::ExportRecovery => self.recovery.export_applicability(),
            A::Quit => self.recovery.quit_applicability(),
            A::Undo => self.history_applicability(self.history.can_undo, "Nothing to undo"),
            A::Redo => self.history_applicability(self.history.can_redo, "Nothing to redo"),
            A::Attachments => self.attachments_applicability(),
            A::InstalledHighlights => Self::when(
                self.features.installed_highlights,
                "Unavailable for this Proqi installation",
            ),
        }
    }

    pub(super) fn relevance(&self, metadata: CommandMetadata) -> Option<u8> {
        if !self.applicability(metadata).enabled {
            return None;
        }
        match metadata.relevance {
            CommandRelevance::Always(priority) => Some(priority),
            CommandRelevance::FocusedThought(priority) if self.board.focused => Some(priority),
            CommandRelevance::Selection(priority) if self.selection.count >= 2 => Some(priority),
            CommandRelevance::Editor(priority) if self.selection.editor_handoff.is_some() => {
                Some(priority)
            }
            CommandRelevance::Submission(priority) if self.features.submit_supported => {
                Some(priority)
            }
            CommandRelevance::Undo(priority) if self.history.can_undo => Some(priority),
            CommandRelevance::Redo(priority) if self.history.can_redo => Some(priority),
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
            | CommandRelevance::Undo(_)
            | CommandRelevance::Redo(_)
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
        if !self.board.focused {
            Applicability::disabled("No thought is focused")
        } else if self.recovery.failed() {
            Applicability::disabled("Resolve the failed save first")
        } else if !self.mutation.focused_mutable {
            Applicability::disabled("Thought has an operation in progress")
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
        let mutable = self.when_mutable();
        if !mutable.enabled {
            return mutable;
        }
        if self.selection.count > 1 {
            return Applicability::disabled("Unavailable for multiple selected thoughts");
        }
        Self::when(self.board.live_thought_count > 1, "Nothing to reorder")
    }

    const fn board_nonempty_applicability(&self) -> Applicability {
        if !self.has_live_thoughts() {
            Applicability::disabled("Board has no thoughts")
        } else if !matches!(self.board.mode, InteractionMode::Board) {
            Applicability::disabled("Available from Board focus")
        } else {
            Applicability::ENABLED
        }
    }

    const fn history_applicability(
        &self,
        available: bool,
        empty_reason: &'static str,
    ) -> Applicability {
        if self.recovery.failed() {
            Applicability::disabled("Resolve the failed save first")
        } else if self.board.operation_pending {
            Applicability::disabled("A board operation is still being saved")
        } else {
            Self::when(available, empty_reason)
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
        if self.selection.count < 2 {
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
            && !self.board.insertion_focused
            && self.board.focused
    }
}

impl BoardApp {
    pub(super) fn capture_recovery_command_context(&self) -> RecoveryContext {
        RecoveryContext::capture(&self.state.durability, self.recovery_exported_for)
    }

    pub(super) fn board_thought_command_available(&self) -> bool {
        matches!(self.state.mode, InteractionMode::Board)
            && !self.insertion_focused()
            && self.state.focused_thought.is_some()
    }

    pub(super) fn capture_screenshot_command_context(&self) -> ScreenshotContext {
        ScreenshotContext {
            action: self.screenshot_palette_action(),
            retry: self.screenshot_retry_ready(),
        }
    }

    pub(super) fn capture_command_context(&mut self) -> CommandContext {
        let selected_ids = self.action_thought_ids();
        let has_thought = self.active_thought_id().is_some();
        let mutable_thought = has_thought
            && !self.state.deferred_board_operation_pending()
            && selected_ids.iter().all(|id| {
                !self.submission_locked(*id)
                    && !self
                        .pending_transfer_removals
                        .values()
                        .any(|pending| pending == id)
            });
        let live_thoughts = self.state.board.live_thoughts();
        let selection_count = self.selection_len();
        let selection_contiguous = selection_is_contiguous(self, &selected_ids, selection_count);
        let merge_handoff = (selection_count >= 2).then(|| {
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
                focused: has_thought,
                mode: self.state.mode,
                insertion_focused: self.insertion_focused(),
                operation_pending: self.state.deferred_board_operation_pending(),
            },
            mutation: MutationContext {
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
            history: HistoryContext {
                can_undo: self.state.can_undo(self.state.mode),
                can_redo: self.state.can_redo(self.state.mode),
            },
            recovery: self.capture_recovery_command_context(),
            attachments: AttachmentContext {
                present: has_attachments,
                refreshing: self.state.attachments.manual_refresh_active(),
            },
            screenshot: self.capture_screenshot_command_context(),
            selection: SelectionContext {
                count: selection_count,
                contiguous: selection_contiguous,
                editor_handoff: self.palette_selection_handoff.take(),
                merge_handoff,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::{editor::RopeEditorFactory, memory::FakeIdGenerator},
        application::{AppState, DurabilityState, FailureCode},
        domain::{OperationSequence, Session, SessionBoard, Timestamp},
        ports::environment::IdGenerator as _,
        ui::{ShortcutActionId, UiSettings},
    };

    fn empty_app() -> BoardApp {
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-commands-context"),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let board = SessionBoard::new(session, Vec::new()).expect("board");
        BoardApp::with_settings(
            AppState::new(board),
            UiSettings::default(),
            RopeEditorFactory,
        )
    }

    fn metadata(app: &BoardApp, action: ShortcutActionId) -> CommandMetadata {
        app.settings
            .shortcuts
            .descriptor(action)
            .and_then(|descriptor| descriptor.commands)
            .expect("Commands metadata")
    }

    #[test]
    fn recovery_actions_follow_durable_pending_and_failed_state_exactly() {
        let mut app = empty_app();
        let retry = metadata(&app, ShortcutActionId::RetryStorage);
        let export = metadata(&app, ShortcutActionId::ExportRecovery);
        let undo = metadata(&app, ShortcutActionId::Undo);

        let durable = app.capture_command_context();
        assert_eq!(
            durable.applicability(retry),
            Applicability::disabled("Available after a save failure")
        );

        app.state.durability = DurabilityState::Pending {
            durable: OperationSequence::ZERO,
            latest: OperationSequence::new(1),
        };
        let pending = app.capture_command_context();
        assert_eq!(
            pending.applicability(export),
            Applicability::disabled("Available after a save failure")
        );

        app.state.durability = DurabilityState::Failed {
            durable: OperationSequence::ZERO,
            failed: OperationSequence::new(1),
            code: FailureCode::StorageFailed,
        };
        let failed = app.capture_command_context();
        assert_eq!(failed.applicability(retry), Applicability::ENABLED);
        assert_eq!(failed.applicability(export), Applicability::ENABLED);
        assert_eq!(
            failed.applicability(undo),
            Applicability::disabled("Resolve the failed save first")
        );
        assert_eq!(failed.relevance(retry), Some(0));
    }
}
