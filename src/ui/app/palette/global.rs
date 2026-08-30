//! Command-palette actions that do not need editor or submission handoff state.

use crate::{
    application::{Effect, InteractionMode, UpdateIntent},
    ports::environment::{Clock, IdGenerator},
};

use super::{super::BoardApp, command::Command};

impl BoardApp {
    pub(super) fn execute_global_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            Command::New => self.create(crate::ui::PastePayload::text(String::new()), ids, clock),
            Command::RenameSession => {
                self.begin_session_rename();
                Vec::new()
            }
            Command::CopySessionId => self.copy_session_id(ids),
            Command::CopyResume => self.copy_resume_command(ids),
            Command::SendSession => self.begin_session_transfer(false, ids, clock),
            Command::SendSessionRemove => self.begin_session_transfer(true, ids, clock),
            Command::Delete => self.delete(ids, clock),
            Command::Copy => self.copy_active(ids),
            Command::Cut => self.cut_active(ids, clock),
            Command::Paste => self.read_clipboard(ids),
            Command::Duplicate => self.duplicate(ids, clock),
            Command::SelectAll => self.select_all_from_palette(ids, clock),
            Command::RefreshAgents => self.refresh_agents(),
            Command::RefreshInvocations => self.refresh_invocations(),
            Command::CheckUpdates => vec![Effect::Update(UpdateIntent::CheckNow)],
            Command::ScreenshotInbox => self.toggle_screenshot_inbox(ids, clock),
            Command::RetryScreenshotCapture => self.retry_screenshot_capture(ids, clock),
            Command::RetryStorage => self.retry_persistence(),
            Command::ExportRecovery => self.export_recovery(ids, clock),
            Command::Undo => self.history(ids, clock, true),
            Command::Redo => self.history(ids, clock, false),
            Command::MoveUp => self.reorder(ids, clock, -1),
            Command::MoveDown => self.reorder(ids, clock, 1),
            Command::Collapse => self.collapse(ids, clock),
            Command::Select => {
                self.toggle_selection();
                Vec::new()
            }
            Command::RangeSelect => {
                self.activate_range_latch();
                Vec::new()
            }
            Command::Help => {
                self.help = true;
                Vec::new()
            }
            Command::Quit => {
                self.request_quit();
                Vec::new()
            }
            Command::SubmitRemove
            | Command::SubmitKeep
            | Command::SubmitAllRemove
            | Command::SubmitAllKeep
            | Command::PlainNewline
            | Command::JumpUp
            | Command::JumpDown
            | Command::ThoughtStart
            | Command::ThoughtEnd
            | Command::Indent
            | Command::Outdent
            | Command::SplitThought
            | Command::ExtractSelection
            | Command::MergeThoughts
            | Command::Edit
            | Command::InsertInvocation => Vec::new(),
        }
    }

    fn select_all_from_palette(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let effects = if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            self.finish_edit(ids, clock)
        } else {
            Vec::new()
        };
        self.select_all_thoughts();
        effects
    }
}
