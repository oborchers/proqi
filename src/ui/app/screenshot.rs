//! Screenshot inbox UI state and commit-first queueing.

use std::collections::VecDeque;

use crate::{
    application::{DurabilityState, Effect, ScreenshotIntent, apply_capture, prepare_capture},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::CaptureOwnerInfo,
        screenshot::{ScreenshotCandidate, ScreenshotError},
        store::{CaptureCommitOutcome, StoreError},
    },
};

use super::BoardApp;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ScreenshotState {
    #[default]
    Off,
    Starting,
    Listening,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ScreenshotSave {
    Ready(crate::ports::store::CaptureCommit),
    InFlight(crate::ports::store::CaptureCommit),
}

#[derive(Default)]
pub(super) struct ScreenshotInbox {
    state: ScreenshotState,
    candidates: VecDeque<ScreenshotCandidate>,
    save: Option<ScreenshotSave>,
    pub(super) takeover: Option<CaptureOwnerInfo>,
    pub(super) takeover_selected: usize,
    auto_ready: Option<crate::domain::ThoughtId>,
    pub(super) notice_count: usize,
}

impl BoardApp {
    pub(super) fn handle_screenshot_commit_barrier(
        &mut self,
        input: crate::ui::input::UiInput,
    ) -> Vec<Effect> {
        use crate::{
            ports::editor::EditCommand,
            ui::input::{UiInput, UiKey},
        };
        match input {
            UiInput::Key(UiKey::PrimaryShiftMove { movement })
                if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) =>
            {
                self.apply_edit(EditCommand::Move {
                    movement,
                    extend_selection: true,
                });
            }
            UiInput::Key(key)
                if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) =>
            {
                if let Some((command, _)) = super::editing::command_for_key(key, false) {
                    self.apply_edit(command);
                }
            }
            UiInput::Paste(content)
                if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) =>
            {
                self.apply_edit(EditCommand::Paste(content));
            }
            UiInput::PasteAnnotated(payload)
                if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) =>
            {
                self.apply_annotated_edit(
                    EditCommand::Paste(payload.content),
                    &payload.annotations,
                );
            }
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
                self.edit_boundary = None;
            }
            UiInput::HostFocusGained
            | UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    pub(super) fn toggle_screenshot_inbox(&mut self) -> Vec<Effect> {
        if !self.screenshot.candidates.is_empty() {
            self.set_info("Screenshot Inbox is finishing captured files");
            return Vec::new();
        }
        if let Some(ScreenshotSave::Ready(commit)) = self.screenshot.save.clone() {
            self.screenshot.save = Some(ScreenshotSave::InFlight(commit.clone()));
            self.set_info("Retrying Screenshot Inbox save");
            return vec![Effect::CommitCapture(commit)];
        }
        match self.screenshot.state {
            ScreenshotState::Off => {
                self.screenshot.state = ScreenshotState::Starting;
                vec![Effect::Screenshot(ScreenshotIntent::Enable)]
            }
            ScreenshotState::Starting | ScreenshotState::Listening => {
                self.screenshot.state = ScreenshotState::Stopping;
                vec![Effect::Screenshot(ScreenshotIntent::Disable)]
            }
            ScreenshotState::Stopping => Vec::new(),
        }
    }

    pub(crate) fn screenshot_started(&mut self) {
        self.screenshot.state = ScreenshotState::Listening;
        self.screenshot.takeover = None;
        self.set_info("Screenshot Inbox is listening");
    }

    pub(crate) fn screenshot_stopped(&mut self) {
        self.screenshot.state = ScreenshotState::Off;
        self.screenshot.takeover = None;
        self.set_info("Screenshot Inbox is disabled");
    }

    pub(crate) fn screenshot_failed(&mut self, error: &ScreenshotError) {
        self.screenshot.state = ScreenshotState::Off;
        self.screenshot.takeover = None;
        self.set_error(error.to_string());
    }

    pub(crate) fn screenshot_conflict(&mut self, owner: CaptureOwnerInfo) {
        self.screenshot.state = ScreenshotState::Off;
        self.screenshot.takeover = Some(owner);
        self.screenshot.takeover_selected = 0;
        self.set_warning("Screenshot Inbox is listening in another Proqi session");
    }

    pub(crate) fn queue_screenshot_candidates(
        &mut self,
        candidates: impl IntoIterator<Item = ScreenshotCandidate>,
    ) {
        if self.screenshot.save.is_none() && self.screenshot.candidates.is_empty() {
            self.screenshot.notice_count = 0;
        }
        self.screenshot.candidates.extend(candidates);
    }

    pub(crate) fn advance_screenshot_capture(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.screenshot.save.is_some()
            || self.pending_edit.is_some()
            || !matches!(self.state.durability, DurabilityState::Durable { .. })
        {
            return Vec::new();
        }
        let Some(candidate) = self.screenshot.candidates.pop_front() else {
            return Vec::new();
        };
        match prepare_capture(
            &self.state,
            &candidate,
            ids.thought_id(),
            ids.operation_id(),
            clock.now(),
        ) {
            Ok(commit) => {
                self.screenshot.save = Some(ScreenshotSave::InFlight(commit.clone()));
                vec![Effect::CommitCapture(commit)]
            }
            Err(error) => {
                self.set_error(error.to_string());
                Vec::new()
            }
        }
    }

    pub(crate) fn complete_screenshot_capture(
        &mut self,
        result: Result<CaptureCommitOutcome, StoreError>,
    ) {
        let Some(ScreenshotSave::InFlight(commit)) = self.screenshot.save.take() else {
            self.set_error("Screenshot Inbox received an unexpected durable result");
            return;
        };
        let was_editing = matches!(
            self.state.mode,
            crate::application::InteractionMode::Edit { .. }
        );
        let advance_auto_ready = self.screenshot.auto_ready.is_some_and(|thought_id| {
            self.pending_edit.is_none()
                && self.state.mode == crate::application::InteractionMode::Edit { thought_id }
        });
        match result {
            Ok(outcome) => match apply_capture(&mut self.state, &commit, &outcome) {
                Ok(Some(thought_id)) => {
                    let make_ready = !was_editing || advance_auto_ready;
                    if advance_auto_ready {
                        self.state.focused_thought = Some(thought_id);
                        self.state.mode = crate::application::InteractionMode::Edit { thought_id };
                    }
                    self.screenshot.auto_ready = (make_ready
                        && !self.screenshot.candidates.is_empty())
                    .then_some(thought_id);
                    self.sync_editor_from_state();
                    self.screenshot.notice_count = self.screenshot.notice_count.saturating_add(1);
                    let count = self.screenshot.notice_count;
                    self.set_success(if count == 1 {
                        "1 new capture".to_owned()
                    } else {
                        format!("{count} new captures")
                    });
                }
                Ok(None) => {
                    if self.screenshot.candidates.is_empty() {
                        self.screenshot.auto_ready = None;
                    }
                }
                Err(error) => {
                    self.screenshot.auto_ready = None;
                    self.set_error(error.to_string());
                }
            },
            Err(error) => {
                self.screenshot.save = Some(ScreenshotSave::Ready(commit));
                self.set_error(format!(
                    "Screenshot Inbox could not save the capture: {error}; it remains retryable"
                ));
            }
        }
    }

    #[must_use]
    /// Whether the installation-wide watcher is actively listening.
    pub const fn screenshot_listening(&self) -> bool {
        matches!(self.screenshot.state, ScreenshotState::Listening)
    }

    #[must_use]
    /// Whether one commit-first capture is retained or in flight.
    pub const fn screenshot_commit_pending(&self) -> bool {
        self.screenshot.save.is_some()
    }

    #[must_use]
    pub(crate) fn screenshot_has_durable_work(&self) -> bool {
        self.screenshot.save.is_some() || !self.screenshot.candidates.is_empty()
    }

    #[must_use]
    pub(super) const fn screenshot_save_in_flight(&self) -> bool {
        matches!(self.screenshot.save, Some(ScreenshotSave::InFlight(_)))
    }

    #[must_use]
    pub(super) const fn screenshot_enabled_for_palette(&self) -> bool {
        !matches!(self.screenshot.state, ScreenshotState::Off)
    }

    pub(super) fn handle_screenshot_takeover_input(
        &mut self,
        input: &crate::ui::input::UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        use crate::{
            ports::editor::CursorMovement,
            ui::input::{UiInput, UiKey},
        };
        match input {
            UiInput::Key(UiKey::Escape) => self.cancel_screenshot_takeover(),
            UiInput::Key(UiKey::Enter) => return self.choose_screenshot_takeover(ids),
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            }) => self.screenshot.takeover_selected = 0,
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualDown,
                ..
            }) => self.screenshot.takeover_selected = 1,
            UiInput::Pointer(pointer) => return self.handle_pointer(*pointer, ids, clock),
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    pub(super) fn choose_screenshot_takeover(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        if self.screenshot.takeover_selected == 0 {
            self.cancel_screenshot_takeover();
            return Vec::new();
        }
        let Some(owner) = self.screenshot.takeover.take() else {
            return Vec::new();
        };
        self.screenshot.state = ScreenshotState::Starting;
        vec![Effect::Screenshot(ScreenshotIntent::TakeOver {
            owner,
            request_id: ids.request_id(),
        })]
    }

    pub(super) fn cancel_screenshot_takeover(&mut self) {
        self.screenshot.takeover = None;
        self.screenshot.takeover_selected = 0;
    }

    pub(in crate::ui) fn screenshot_takeover_view(&self) -> Option<(Vec<String>, usize)> {
        self.screenshot.takeover.as_ref().map(|_| {
            (
                vec!["Cancel".to_owned(), "Take over".to_owned()],
                self.screenshot.takeover_selected,
            )
        })
    }
}

#[cfg(test)]
#[path = "screenshot/tests.rs"]
mod snapshot_tests;
