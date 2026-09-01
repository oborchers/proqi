//! Command-palette dispatch grouped by application capability.

use crate::{
    application::Effect,
    ports::{
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, Command, EditorSelectionHandoff, UiKey};

impl BoardApp {
    pub(super) fn execute_selection_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        match command {
            Command::SelectAll => {
                let effects = if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) {
                    self.finish_edit(ids, clock)
                } else {
                    Vec::new()
                };
                if self.pending_edit.is_some() {
                    return Some(effects);
                }
                self.select_all_thoughts();
                Some(effects)
            }
            Command::Select => {
                self.toggle_selection();
                Some(Vec::new())
            }
            Command::RangeSelect => {
                self.activate_range_latch();
                Some(Vec::new())
            }
            _ => None,
        }
    }

    pub(super) fn execute_runtime_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        match command {
            Command::RefreshAgents => Some(self.refresh_agents()),
            Command::RefreshAttachments => Some(self.refresh_attachments(true)),
            Command::RefreshInvocations => Some(self.refresh_invocations()),
            Command::CheckUpdates => Some(vec![Effect::Update(
                crate::application::UpdateIntent::CheckNow,
            )]),
            Command::ScreenshotInbox => Some(self.toggle_screenshot_inbox(ids, clock)),
            Command::RetryScreenshotCapture => Some(self.retry_screenshot_capture(ids, clock)),
            Command::RetryStorage => Some(self.retry_persistence()),
            Command::ExportRecovery => Some(self.export_recovery(ids, clock)),
            _ => None,
        }
    }

    pub(super) fn execute_entry_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        match command {
            Command::Edit => Some(self.expand_and_enter_edit(ids, clock)),
            Command::InsertInvocation => {
                let effects =
                    if matches!(self.state.mode, crate::application::InteractionMode::Board) {
                        self.expand_and_enter_edit(ids, clock)
                    } else {
                        Vec::new()
                    };
                let mut effects = effects;
                effects.extend(self.open_invocation_picker());
                Some(effects)
            }
            _ => None,
        }
    }

    pub(super) fn execute_submission_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        use crate::ports::agent::SubmissionDisposition::{Keep, RemoveAfterSuccess};
        match command {
            Command::SubmitRemove => Some(self.begin_delivery(RemoveAfterSuccess, ids, clock)),
            Command::SubmitKeep => Some(self.begin_delivery(Keep, ids, clock)),
            Command::SubmitAllRemove => {
                Some(self.begin_delivery_all(RemoveAfterSuccess, ids, clock))
            }
            Command::SubmitAllKeep => Some(self.begin_delivery_all(Keep, ids, clock)),
            _ => None,
        }
    }

    pub(super) fn execute_editor_command(
        &mut self,
        command: Command,
        selection_handoff: Option<EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        if !matches!(
            command,
            Command::PlainNewline
                | Command::JumpUp
                | Command::JumpDown
                | Command::ThoughtStart
                | Command::ThoughtEnd
                | Command::Indent
                | Command::Outdent
        ) {
            return None;
        }
        let mut effects = if matches!(
            self.state.mode,
            crate::application::InteractionMode::Edit { .. }
        ) {
            Vec::new()
        } else {
            self.expand_and_enter_edit(ids, clock)
        };
        self.restore_palette_selection_handoff(selection_handoff);
        if command == Command::PlainNewline {
            effects.extend(self.insert_newline(false, ids, clock));
            return Some(effects);
        }
        let movement = match command {
            Command::JumpUp => Some(CursorMovement::VisualJumpUp),
            Command::JumpDown => Some(CursorMovement::VisualJumpDown),
            Command::ThoughtStart => Some(CursorMovement::DocumentStart),
            Command::ThoughtEnd => Some(CursorMovement::DocumentEnd),
            _ => None,
        };
        if let Some(movement) = movement {
            effects.extend(self.handle_edit_key(
                UiKey::Move {
                    movement,
                    extend_selection: false,
                },
                ids,
                clock,
            ));
            return Some(effects);
        }
        effects.extend(self.apply_indentation(command == Command::Outdent, ids, clock));
        Some(effects)
    }
}
