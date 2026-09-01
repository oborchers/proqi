//! Runtime-backed command-palette actions.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::{super::BoardApp, command::Command};

impl BoardApp {
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
            Command::WhatsNew => Some(self.open_installed_release_highlights()),
            Command::ScreenshotInbox => Some(self.toggle_screenshot_inbox(ids, clock)),
            Command::RetryScreenshotCapture => Some(self.retry_screenshot_capture(ids, clock)),
            Command::RetryStorage => Some(self.retry_persistence()),
            Command::ExportRecovery => Some(self.export_recovery(ids, clock)),
            _ => None,
        }
    }
}
