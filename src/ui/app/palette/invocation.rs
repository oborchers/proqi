//! Palette-independent command availability and captured execution inputs.

use crate::{
    domain::Thought,
    ui::{CommandAvailability, CommandLabel, app::screenshot::ScreenshotPaletteAction},
};

use super::super::{BoardApp, palette_handoff::EditorSelectionHandoff};

pub(super) struct CommandInvocation {
    submit_supported: bool,
    plain_newline_supported: bool,
    screenshot_action: ScreenshotPaletteAction,
    screenshot_retry: bool,
    selection_handoff: Option<EditorSelectionHandoff>,
    merge_handoff: Option<Vec<Thought>>,
}

impl CommandInvocation {
    pub(super) fn available(&self, availability: CommandAvailability) -> bool {
        match availability {
            CommandAvailability::Always => true,
            CommandAvailability::Submission => self.submit_supported,
            CommandAvailability::Editor => self.plain_newline_supported,
            CommandAvailability::ScreenshotRetry => self.screenshot_retry,
            CommandAvailability::Split => self.selection_handoff.is_some(),
            CommandAvailability::Extract => self
                .selection_handoff
                .as_ref()
                .is_some_and(EditorSelectionHandoff::has_selection),
            CommandAvailability::Merge => self.merge_handoff.is_some(),
            CommandAvailability::ScreenshotInbox => {
                self.screenshot_action != ScreenshotPaletteAction::Unavailable
            }
            CommandAvailability::QueryUndo | CommandAvailability::QueryRedo => false,
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
            } => match self.screenshot_action {
                ScreenshotPaletteAction::Enable => enable,
                ScreenshotPaletteAction::Disable => disable,
                ScreenshotPaletteAction::Resume => resume,
                ScreenshotPaletteAction::Unavailable => unavailable,
            },
        }
    }

    pub(super) fn set_screenshot_action(&mut self, action: ScreenshotPaletteAction) {
        self.screenshot_action = action;
    }

    pub(super) fn take_selection_handoff(&mut self) -> Option<EditorSelectionHandoff> {
        self.selection_handoff.take()
    }

    pub(super) fn take_merge_handoff(&mut self) -> Option<Vec<Thought>> {
        self.merge_handoff.take()
    }
}

impl BoardApp {
    pub(super) fn capture_command_invocation(&mut self) -> CommandInvocation {
        let merge_handoff = (self.selection_len() >= 2).then(|| {
            self.action_thought_ids()
                .into_iter()
                .filter_map(|id| self.state.board.thought(id).cloned())
                .collect()
        });
        CommandInvocation {
            submit_supported: self.supports_submission(),
            plain_newline_supported: !self.insertion_focused()
                && self.state.focused_thought.is_some(),
            screenshot_action: self.screenshot_palette_action(),
            screenshot_retry: self.screenshot_retry_ready(),
            selection_handoff: self.palette_selection_handoff.take(),
            merge_handoff,
        }
    }
}
