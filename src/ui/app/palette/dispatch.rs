//! Command-palette dispatch grouped by application capability.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::BoardApp;
use crate::ui::shortcut_registry::{
    PaletteEntryCommand as EntryCommand, PaletteRuntimeCommand as RuntimeCommand,
    PaletteSelectionCommand as SelectionCommand, PaletteSubmissionCommand as SubmissionCommand,
};

impl BoardApp {
    pub(super) fn execute_selection_command(
        &mut self,
        command: SelectionCommand,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            SelectionCommand::SelectAll => {
                let effects = if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) {
                    self.finish_edit(ids, clock)
                } else {
                    Vec::new()
                };
                if self.pending_edit.is_some() {
                    return effects;
                }
                self.select_all_items();
                effects
            }
            SelectionCommand::Select => {
                self.toggle_selection();
                Vec::new()
            }
            SelectionCommand::RangeSelect => {
                self.activate_range_latch();
                Vec::new()
            }
        }
    }

    pub(super) fn execute_runtime_command(
        &mut self,
        command: RuntimeCommand,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            RuntimeCommand::RefreshAgents => self.refresh_agents(),
            RuntimeCommand::RefreshAttachments => self.refresh_attachments(true),
            RuntimeCommand::RefreshInvocations => self.refresh_invocations(),
            RuntimeCommand::CheckUpdates => {
                vec![Effect::Update(crate::application::UpdateIntent::CheckNow)]
            }
            RuntimeCommand::WhatsNew => self.open_installed_release_highlights(),
            RuntimeCommand::ToggleFooter => self.toggle_footer_chrome_visibility(),
            RuntimeCommand::ScreenshotInbox => self.toggle_screenshot_inbox(ids, clock),
            RuntimeCommand::RetryScreenshotCapture => self.retry_screenshot_capture(ids, clock),
            RuntimeCommand::RetryStorage => self.retry_persistence(),
            RuntimeCommand::ExportRecovery => self.export_recovery(ids, clock),
        }
    }

    fn toggle_footer_chrome_visibility(&mut self) -> Vec<Effect> {
        self.footer_chrome_visibility = match self.footer_chrome_visibility {
            crate::ui::layout::FooterChromeVisibility::Visible => {
                crate::ui::layout::FooterChromeVisibility::Hidden
            }
            crate::ui::layout::FooterChromeVisibility::Hidden => {
                crate::ui::layout::FooterChromeVisibility::Visible
            }
        };
        self.layout = None;
        self.frame_presentation = None;
        self.hovered = None;
        Vec::new()
    }

    pub(super) fn execute_entry_command(
        &mut self,
        command: EntryCommand,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            EntryCommand::Edit => self.expand_and_enter_edit(ids, clock),
            EntryCommand::InsertInvocation => {
                let effects =
                    if matches!(self.state.mode, crate::application::InteractionMode::Board) {
                        self.expand_and_enter_edit(ids, clock)
                    } else {
                        Vec::new()
                    };
                let mut effects = effects;
                effects.extend(self.open_invocation_picker());
                effects
            }
        }
    }

    pub(super) fn execute_submission_command(
        &mut self,
        command: SubmissionCommand,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        use crate::ports::agent::SubmissionDisposition::{Keep, RemoveAfterSuccess};
        match command {
            SubmissionCommand::ToAgent => self.begin_global_delivery(ids, clock),
            SubmissionCommand::Remove => self.begin_delivery(RemoveAfterSuccess, ids, clock),
            SubmissionCommand::Keep => self.begin_delivery(Keep, ids, clock),
            SubmissionCommand::AllRemove => self.begin_delivery_all(RemoveAfterSuccess, ids, clock),
            SubmissionCommand::AllKeep => self.begin_delivery_all(Keep, ids, clock),
        }
    }
}
