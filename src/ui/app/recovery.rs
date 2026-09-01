//! Visible retry and safe export controls for failed persistence.

use std::path::PathBuf;

use crate::{
    application::{Action, DurabilityState, Effect, capture_recovery},
    domain::RequestId,
    ports::environment::{Clock, IdGenerator},
};

use super::{BoardApp, UiInput, UiKey, pending_types::EditFlush};

impl BoardApp {
    pub(super) fn handle_quit_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        if matches!(input, UiInput::Key(UiKey::Quit)) && self.screenshot_retry_ready() {
            return Some(self.handle_ready_capture_quit(ids, clock));
        }
        if !matches!(input, UiInput::Key(UiKey::Quit)) && !self.is_failed_recovery_quit(input) {
            return None;
        }
        let flush = if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            EditFlush::Complete(Vec::new())
        } else {
            self.flush_edit_boundary(ids, clock)
        };
        let effects = match flush {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return Some(effects),
        };
        self.request_quit();
        Some(effects)
    }

    pub(super) fn is_failed_recovery_quit(&self, input: &UiInput) -> bool {
        matches!(self.state.durability, DurabilityState::Failed { .. })
            && (matches!(
                input,
                UiInput::Key(UiKey::Character(character))
                    if *character == self.settings.keybindings.quit
            ) || matches!(input, UiInput::Key(UiKey::UnmodifiedSpace))
                && self.settings.keybindings.quit == ' ')
    }

    pub(super) fn handle_failed_recovery_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        if !matches!(self.state.durability, DurabilityState::Failed { .. }) {
            return None;
        }
        match input {
            UiInput::Key(UiKey::Character(crate::ui::settings::RECOVERY_RETRY_KEY)) => {
                Some(self.retry_persistence())
            }
            UiInput::Key(UiKey::Character(crate::ui::settings::RECOVERY_EXPORT_KEY)) => {
                Some(self.export_recovery(ids, clock))
            }
            UiInput::Pointer(pointer) => Some(self.handle_recovery_pointer(*pointer, ids, clock)),
            UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::HostFocusLost => None,
            UiInput::Key(_) | UiInput::Paste(_) | UiInput::PasteAnnotated(_) => Some(Vec::new()),
        }
    }

    pub(super) fn export_recovery(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if !matches!(self.state.durability, DurabilityState::Failed { .. }) {
            self.set_warning("recovery export is available after a save failure");
            return Vec::new();
        }
        let request_id = ids.request_id();
        let exported_at = clock.now();
        let mut document = capture_recovery(&self.state, exported_at);
        if let Some((thought_id, snapshot)) = self.pending_edit_snapshot()
            && let Some(thought) = document
                .thoughts
                .iter_mut()
                .find(|thought| thought.id == thought_id)
        {
            thought.content.clone_from(&snapshot.content);
            thought.updated_at = exported_at;
        }
        self.pending_recovery_exports.insert(request_id);
        self.set_info("exporting recovery file");
        vec![Effect::ExportRecovery {
            request_id,
            document: Box::new(document),
        }]
    }

    /// Complete one recovery write on the UI lane.
    pub fn complete_recovery_export(
        &mut self,
        request_id: RequestId,
        result: Result<PathBuf, String>,
    ) -> Vec<Effect> {
        if !self.pending_recovery_exports.remove(&request_id) {
            return Vec::new();
        }
        match result {
            Ok(path) => {
                self.recovery_exported_for = match self.state.durability {
                    DurabilityState::Failed { failed, .. } => Some(failed),
                    DurabilityState::Durable { .. } | DurabilityState::Pending { .. } => None,
                };
                self.set_success(format!("recovery exported to {}", path.display()));
            }
            Err(error) => self.set_error(format!("recovery export failed: {error}")),
        }
        Vec::new()
    }

    pub(super) fn retry_persistence(&mut self) -> Vec<Effect> {
        let DurabilityState::Failed { failed, code, .. } = self.state.durability else {
            return Vec::new();
        };
        if code == crate::application::FailureCode::RecoveryCapacity {
            self.set_error("retry is unavailable; export recovery before quitting");
            return Vec::new();
        }
        self.reduce(Action::RetryPersistence(failed))
    }
}
