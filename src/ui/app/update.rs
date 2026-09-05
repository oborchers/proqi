//! Temporary update-barrier state around ordinary durable session resume.

use crate::{
    application::{DurabilityState, Effect, UpdateIntent},
    domain::{InstallationKind, RequestId, StableVersion, Timestamp},
    ui::{HitTarget, ListNavigation, PointerButton, PointerKind, UiInput, UiKey},
};

use super::BoardApp;

pub(super) struct UpdateBarrier {
    operation_id: RequestId,
    deadline: Timestamp,
    reserved_restart: Option<StableVersion>,
}

pub(super) struct UpdatePrompt {
    version: StableVersion,
    installation: InstallationKind,
    participants: usize,
    selected: usize,
    input_boundary: u64,
    armed: bool,
}

impl BoardApp {
    #[cfg(test)]
    pub(crate) fn present_update(
        &mut self,
        version: StableVersion,
        installation: InstallationKind,
        participants: usize,
    ) {
        self.install_update_prompt(version, installation, participants, 0, true);
    }

    pub(crate) fn present_update_protected(
        &mut self,
        version: StableVersion,
        installation: InstallationKind,
        participants: usize,
        input_boundary: u64,
    ) {
        self.install_update_prompt(version, installation, participants, input_boundary, false);
    }

    fn install_update_prompt(
        &mut self,
        version: StableVersion,
        installation: InstallationKind,
        participants: usize,
        input_boundary: u64,
        armed: bool,
    ) {
        if installation == InstallationKind::SourceOrUnknown {
            return;
        }
        self.help = false;
        self.palette = None;
        self.search = None;
        self.rename = None;
        self.transfer = None;
        self.update_prompt = Some(UpdatePrompt {
            version,
            installation,
            participants,
            selected: 1,
            input_boundary,
            armed,
        });
        self.layout = None;
    }

    pub(crate) fn arm_update_prompt(&mut self) {
        if let Some(prompt) = &mut self.update_prompt {
            prompt.armed = true;
        }
    }

    pub(crate) fn accept_update_input(&self, sequence: u64) -> bool {
        self.update_prompt.as_ref().is_none_or(|prompt| {
            prompt.armed && (sequence == 0 || sequence > prompt.input_boundary)
        })
    }

    pub(super) fn handle_update_prompt_input(&mut self, input: &UiInput) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.choose_update(1),
            UiInput::Key(UiKey::Enter) => {
                let selected = self
                    .update_prompt
                    .as_ref()
                    .map_or(0, |prompt| prompt.selected);
                self.choose_update(selected)
            }
            UiInput::Key(UiKey::FastNavigation { direction, .. }) => {
                self.move_update_selection(direction.delta());
                Vec::new()
            }
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Previous) => {
                self.move_update_selection(-1);
                Vec::new()
            }
            UiInput::Key(key) if key.list_navigation() == Some(ListNavigation::Next) => {
                self.move_update_selection(1);
                Vec::new()
            }
            UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) =>
            {
                match self
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.hit_test(pointer.column, pointer.row))
                {
                    Some(HitTarget::PaletteItem(index)) => self.choose_update(index),
                    Some(HitTarget::CloseOverlay) => self.choose_update(1),
                    _ => Vec::new(),
                }
            }
            UiInput::Pointer(pointer) if matches!(pointer.kind, PointerKind::ScrollUp) => {
                self.move_update_selection(-1);
                Vec::new()
            }
            UiInput::Pointer(pointer) if matches!(pointer.kind, PointerKind::ScrollDown) => {
                self.move_update_selection(1);
                Vec::new()
            }
            UiInput::Resize { .. } => {
                self.layout = None;
                Vec::new()
            }
            UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_)
            | UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_) => Vec::new(),
        }
    }

    pub(in crate::ui) fn update_prompt_view(&self) -> Option<(String, Vec<String>, usize)> {
        self.update_prompt.as_ref().map(|prompt| {
            let primary = match prompt.installation {
                InstallationKind::HomebrewFormula => format!(
                    "Update and restart all {} {}",
                    prompt.participants,
                    if prompt.participants == 1 {
                        "session"
                    } else {
                        "sessions"
                    }
                ),
                InstallationKind::StandaloneArchive | InstallationKind::SourceOrUnknown => {
                    "View update instructions".to_owned()
                }
            };
            (
                format!(" update available · {} ", prompt.version),
                vec![
                    primary,
                    "Not now".to_owned(),
                    format!("Skip {}", prompt.version),
                ],
                prompt.selected,
            )
        })
    }

    pub(crate) fn complete_update_action(&mut self, result: Result<String, String>) {
        match result {
            Ok(message) => self.set_success(message),
            Err(message) => self.set_error(message),
        }
    }

    fn move_update_selection(&mut self, delta: isize) {
        if let Some(prompt) = &mut self.update_prompt {
            prompt.selected = prompt.selected.saturating_add_signed(delta).min(2);
        }
    }

    fn choose_update(&mut self, index: usize) -> Vec<Effect> {
        let Some(prompt) = self.update_prompt.take() else {
            return Vec::new();
        };
        self.layout = None;
        let intent = match index {
            0 if prompt.installation == InstallationKind::HomebrewFormula => {
                self.set_warning(format!(
                    "Preparing {} Proqi {} for update.",
                    prompt.participants,
                    if prompt.participants == 1 {
                        "session"
                    } else {
                        "sessions"
                    }
                ));
                UpdateIntent::Install(prompt.version)
            }
            0 => UpdateIntent::ViewInstructions(prompt.version),
            2 => UpdateIntent::Skip(prompt.version),
            _ => UpdateIntent::Dismiss(prompt.version),
        };
        vec![Effect::Update(intent)]
    }

    pub(crate) fn begin_update_barrier(
        &mut self,
        operation_id: RequestId,
        deadline: Timestamp,
    ) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_some_and(|barrier| barrier.operation_id != operation_id)
        {
            return false;
        }
        self.update_barrier = Some(UpdateBarrier {
            operation_id,
            deadline,
            reserved_restart: None,
        });
        self.set_warning("Ready for Proqi update. Waiting for all sessions.");
        true
    }

    pub(crate) fn release_update_barrier(&mut self, operation_id: RequestId) -> bool {
        if self.update_barrier.as_ref().is_none_or(|barrier| {
            barrier.operation_id != operation_id || barrier.reserved_restart.is_some()
        }) {
            return false;
        }
        self.update_barrier = None;
        self.set_info("Update cancelled. Session is ready.");
        true
    }

    pub(crate) fn expire_update_barrier(&mut self, now: Timestamp) -> bool {
        if self
            .update_barrier
            .as_ref()
            .is_none_or(|barrier| barrier.reserved_restart.is_some() || now < barrier.deadline)
        {
            return false;
        }
        self.update_barrier = None;
        self.set_warning("Update coordinator timed out. Session is ready.");
        true
    }

    pub(crate) fn reserve_update_restart(
        &mut self,
        operation_id: RequestId,
        installed: StableVersion,
    ) -> bool {
        let Some(barrier) = self.update_barrier.as_mut() else {
            return false;
        };
        if barrier.operation_id != operation_id || barrier.reserved_restart.is_some() {
            return false;
        }
        barrier.reserved_restart = Some(installed);
        true
    }

    pub(crate) fn finish_update_restart_delivery(
        &mut self,
        operation_id: RequestId,
        delivered: bool,
    ) -> bool {
        let Some(barrier) = self.update_barrier.as_mut() else {
            return false;
        };
        if barrier.operation_id != operation_id {
            return false;
        }
        let Some(installed) = barrier.reserved_restart.take() else {
            return false;
        };
        if delivered {
            self.update_restart = Some(installed);
            self.quit = true;
        } else {
            self.update_barrier = None;
        }
        true
    }

    pub(crate) fn update_restart(&self) -> Option<&StableVersion> {
        self.update_restart.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn update_barrier_operation(&self) -> Option<RequestId> {
        self.update_barrier
            .as_ref()
            .map(|barrier| barrier.operation_id)
    }

    pub(crate) fn update_preflight_ready(&self) -> bool {
        self.pending_edit.is_none()
            && matches!(self.state.durability, DurabilityState::Durable { .. })
    }

    pub(crate) fn update_preflight_failed(&self) -> bool {
        matches!(self.state.durability, DurabilityState::Failed { .. })
    }
}

#[cfg(test)]
mod tests;
