//! Screenshot inbox UI state and commit-first queueing.

mod activity;
mod presentation;
mod takeover;

use std::{collections::VecDeque, time::Duration};

use crate::{
    application::{
        DurabilityState, Effect, ScreenshotIntent, ScreenshotPauseReason, apply_capture,
        prepare_capture,
    },
    ports::{
        environment::{Clock, IdGenerator},
        runtime::CaptureOwnerInfo,
        screenshot::{ScreenshotActivityPolicy, ScreenshotCandidate, ScreenshotError},
        store::{CaptureCommitOutcome, StoreError},
    },
};

use super::BoardApp;
use activity::ScreenshotActivity;
use presentation::pause_notice;

const DEFERRED_INPUT_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ScreenshotState {
    #[default]
    Off,
    Starting,
    Listening,
    Stopping,
    Paused(ScreenshotPauseReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScreenshotPaletteAction {
    Enable,
    Disable,
    Resume,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ScreenshotSave {
    Ready(ScreenshotCandidate),
    InFlight {
        candidate: ScreenshotCandidate,
        commit: Box<crate::ports::store::CaptureCommit>,
    },
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
    activity: ScreenshotActivity,
    pending_pause: Option<ScreenshotPauseReason>,
    pause_notice: Option<String>,
    deferred_inputs: VecDeque<crate::ui::UiInput>,
    ready_quit_armed: bool,
}

impl BoardApp {
    pub(crate) const fn configure_screenshot_activity(&mut self, policy: ScreenshotActivityPolicy) {
        self.screenshot.activity.configure(policy);
    }

    pub(crate) fn note_screenshot_activity(&mut self, input: &crate::ui::UiInput, now: Duration) {
        if matches!(self.screenshot.state, ScreenshotState::Listening) {
            self.screenshot.activity.note_input(input, now);
        }
    }

    pub(crate) fn advance_screenshot_activity(&mut self, now: Duration) -> Vec<Effect> {
        if !matches!(self.screenshot.state, ScreenshotState::Listening) {
            return Vec::new();
        }
        self.screenshot
            .activity
            .expired(now)
            .map_or_else(Vec::new, |reason| {
                self.request_screenshot_auto_pause(reason)
            })
    }

    pub(super) fn handle_screenshot_commit_barrier(
        &mut self,
        input: crate::ui::input::UiInput,
    ) -> Vec<Effect> {
        use crate::ui::input::UiInput;
        match input {
            UiInput::HostFocusGained => return Self::discover_agents(),
            deferred if self.screenshot.deferred_inputs.len() < DEFERRED_INPUT_LIMIT => {
                self.screenshot.deferred_inputs.push_back(deferred);
            }
            UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_)
            | UiInput::Resize { .. } => {
                self.set_error(
                    "Screenshot Inbox input queue is full; that input was not accepted—wait for the save result and retry",
                );
            }
        }
        Vec::new()
    }

    fn replay_screenshot_inputs(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let queued = std::mem::take(&mut self.screenshot.deferred_inputs);
        let mut effects = Vec::new();
        for input in queued {
            effects.extend(self.handle(input, ids, clock));
            if self.quit {
                break;
            }
        }
        effects
    }

    pub(super) fn toggle_screenshot_inbox(
        &mut self,
        _ids: &mut impl IdGenerator,
        _clock: &impl Clock,
    ) -> Vec<Effect> {
        match self.screenshot.state {
            ScreenshotState::Off | ScreenshotState::Paused(_) => {
                self.screenshot.pending_pause = None;
                self.screenshot.pause_notice = None;
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

    pub(super) fn retry_screenshot_capture(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(ScreenshotSave::Ready(candidate)) = self.screenshot.save.clone() else {
            self.set_warning("Screenshot Inbox has no failed capture to retry");
            return Vec::new();
        };
        self.prepare_screenshot_save(candidate, ids, clock, true)
    }

    pub(super) fn handle_ready_capture_quit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if !self.screenshot_retry_ready() {
            return Vec::new();
        }
        if self.screenshot.ready_quit_armed {
            self.screenshot.save = None;
            self.screenshot.candidates.clear();
            self.screenshot.ready_quit_armed = false;
            let effects = self.flush_pending_edit(ids, clock);
            self.request_quit();
            return effects;
        }
        self.screenshot.ready_quit_armed = true;
        self.set_error(
            "Screenshot capture is not durable; choose Retry Screenshot Capture, or quit again to abandon the retained capture",
        );
        match self.screenshot.state {
            ScreenshotState::Starting | ScreenshotState::Listening => {
                self.screenshot.state = ScreenshotState::Stopping;
                vec![Effect::Screenshot(ScreenshotIntent::Disable)]
            }
            ScreenshotState::Off | ScreenshotState::Stopping | ScreenshotState::Paused(_) => {
                Vec::new()
            }
        }
    }

    pub(crate) fn screenshot_started(&mut self, now: Duration) {
        self.screenshot.state = ScreenshotState::Listening;
        self.screenshot.takeover = None;
        self.screenshot.pending_pause = None;
        self.screenshot.pause_notice = None;
        self.screenshot.activity.start(now);
        self.set_info("Screenshot Inbox is listening");
    }

    pub(crate) fn screenshot_stopped(&mut self) -> Vec<Effect> {
        if matches!(self.screenshot.state, ScreenshotState::Paused(_)) {
            return Vec::new();
        }
        self.screenshot.takeover = None;
        self.screenshot.activity.stop();
        let Some(reason) = self.screenshot.pending_pause.take() else {
            self.screenshot.state = ScreenshotState::Off;
            if self.screenshot_retry_ready() {
                self.set_error(
                    "Screenshot Inbox is disabled and capture authority was released; choose Retry Screenshot Capture",
                );
            } else {
                self.set_info("Screenshot Inbox is disabled");
            }
            return Vec::new();
        };
        self.enter_screenshot_paused(reason);
        if self.screenshot_retry_ready() {
            self.set_error(
                "Screenshot Inbox paused and released capture authority; choose Retry Screenshot Capture",
            );
        }
        vec![Effect::NotifyScreenshotPause(reason)]
    }

    pub(crate) fn screenshot_failed(&mut self, error: &ScreenshotError) -> Vec<Effect> {
        self.screenshot.takeover = None;
        self.screenshot.activity.stop();
        if let Some(reason) = self.screenshot.pending_pause.take() {
            self.enter_screenshot_paused(reason);
            self.set_error(format!(
                "Screenshot Inbox paused, but final reconciliation failed: {error}"
            ));
            return vec![Effect::NotifyScreenshotPause(reason)];
        }
        if matches!(self.screenshot.state, ScreenshotState::Paused(_)) {
            self.set_error(error.to_string());
            return Vec::new();
        }
        self.screenshot.state = ScreenshotState::Off;
        self.screenshot.pause_notice = None;
        self.set_error(error.to_string());
        Vec::new()
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
    ) -> Vec<Effect> {
        if self.quit
            || !matches!(
                self.screenshot.state,
                ScreenshotState::Listening | ScreenshotState::Stopping
            )
        {
            return Vec::new();
        }
        if self.screenshot.save.is_none() && self.screenshot.candidates.is_empty() {
            self.screenshot.notice_count = 0;
        }
        let remaining = self.screenshot.activity.remaining();
        let accepted = candidates.into_iter().take(remaining).collect::<Vec<_>>();
        let count = accepted.len();
        self.screenshot.candidates.extend(accepted);
        let reason = self.screenshot.activity.admit(count);
        if matches!(self.screenshot.state, ScreenshotState::Listening) {
            return reason.map_or_else(Vec::new, |reason| {
                self.request_screenshot_auto_pause(reason)
            });
        }
        Vec::new()
    }

    fn request_screenshot_auto_pause(&mut self, reason: ScreenshotPauseReason) -> Vec<Effect> {
        self.screenshot.pending_pause = Some(reason);
        self.screenshot.state = ScreenshotState::Stopping;
        vec![Effect::Screenshot(ScreenshotIntent::Disable)]
    }

    fn enter_screenshot_paused(&mut self, reason: ScreenshotPauseReason) {
        self.screenshot.state = ScreenshotState::Paused(reason);
        self.screenshot.pause_notice = Some(pause_notice(reason));
        self.status = None;
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
        self.prepare_screenshot_save(candidate, ids, clock, false)
    }

    fn prepare_screenshot_save(
        &mut self,
        candidate: ScreenshotCandidate,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        retry: bool,
    ) -> Vec<Effect> {
        match prepare_capture(
            &self.state,
            &candidate,
            ids.thought_id(),
            ids.operation_id(),
            clock.now(),
        ) {
            Ok(commit) => {
                self.screenshot.save = Some(ScreenshotSave::InFlight {
                    candidate,
                    commit: Box::new(commit.clone()),
                });
                if retry {
                    self.set_info("Retrying Screenshot Inbox save");
                }
                vec![Effect::CommitCapture(commit)]
            }
            Err(error) => {
                self.screenshot.save = Some(ScreenshotSave::Ready(candidate));
                self.set_error(error.to_string());
                Vec::new()
            }
        }
    }

    pub(crate) fn complete_screenshot_capture(
        &mut self,
        result: Result<CaptureCommitOutcome, StoreError>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(ScreenshotSave::InFlight { candidate, commit }) = self.screenshot.save.take()
        else {
            self.set_error("Screenshot Inbox received an unexpected durable result");
            return Vec::new();
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
            Ok(outcome) => {
                self.apply_completed_capture(&commit, &outcome, was_editing, advance_auto_ready);
            }
            Err(error) => {
                self.capture_save_failed(candidate, &error);
            }
        }
        self.replay_screenshot_inputs(ids, clock)
    }

    fn apply_completed_capture(
        &mut self,
        commit: &crate::ports::store::CaptureCommit,
        outcome: &CaptureCommitOutcome,
        was_editing: bool,
        advance_auto_ready: bool,
    ) {
        match apply_capture(&mut self.state, commit, outcome) {
            Ok(Some(thought_id)) => {
                self.show_created_capture(thought_id, was_editing, advance_auto_ready);
            }
            Ok(None) if self.screenshot.candidates.is_empty() => {
                self.screenshot.auto_ready = None;
            }
            Ok(None) => {}
            Err(error) => {
                self.screenshot.auto_ready = None;
                self.set_error(error.to_string());
            }
        }
    }

    fn show_created_capture(
        &mut self,
        thought_id: crate::domain::ThoughtId,
        was_editing: bool,
        advance_auto_ready: bool,
    ) {
        let make_ready = self.capture_auto_focus_is_safe(was_editing, advance_auto_ready);
        if make_ready {
            self.state.focused_thought = Some(thought_id);
            self.state.mode = crate::application::InteractionMode::Edit { thought_id };
        }
        self.screenshot.auto_ready =
            (make_ready && !self.screenshot.candidates.is_empty()).then_some(thought_id);
        self.sync_editor_from_state();
        self.screenshot.notice_count = self.screenshot.notice_count.saturating_add(1);
        let count = self.screenshot.notice_count;
        let message = if count == 1 {
            "1 new capture".to_owned()
        } else {
            format!("{count} new captures")
        };
        self.set_success(message);
    }

    fn capture_auto_focus_is_safe(&self, was_editing: bool, advance_auto_ready: bool) -> bool {
        let interaction_clear = !self.help
            && self.palette.is_none()
            && self.invocation_popup.is_none()
            && self.search.is_none()
            && self.rename.is_none()
            && self.transfer.is_none()
            && self.update_prompt.is_none()
            && self.screenshot.takeover.is_none()
            && self.submission_mode.is_none()
            && self.selection.is_empty()
            && !self
                .screenshot
                .deferred_inputs
                .iter()
                .any(crate::ui::UiInput::is_deliberate_interaction);
        (advance_auto_ready || !was_editing) && interaction_clear
    }

    fn capture_save_failed(&mut self, candidate: ScreenshotCandidate, error: &StoreError) {
        self.set_error(format!(
            "Screenshot Inbox could not save the capture: {error}; choose Retry Screenshot Capture"
        ));
        self.screenshot.save = Some(ScreenshotSave::Ready(candidate));
    }

    pub(super) fn note_screenshot_interaction(&mut self, input: &crate::ui::input::UiInput) {
        if !matches!(input, crate::ui::UiInput::Key(crate::ui::UiKey::Quit)) {
            self.screenshot.ready_quit_armed = false;
        }
        if input.is_deliberate_interaction() {
            self.screenshot.auto_ready = None;
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
        self.screenshot_sequence_reserved()
    }

    #[must_use]
    pub(crate) const fn screenshot_retry_ready(&self) -> bool {
        matches!(self.screenshot.save, Some(ScreenshotSave::Ready(_)))
    }

    #[must_use]
    pub(crate) fn screenshot_blocks_capture_release(&self) -> bool {
        match &self.screenshot.save {
            Some(ScreenshotSave::InFlight { .. }) => true,
            Some(ScreenshotSave::Ready(_)) => false,
            None => !self.screenshot.candidates.is_empty(),
        }
    }

    #[must_use]
    pub(super) const fn screenshot_save_in_flight(&self) -> bool {
        matches!(self.screenshot.save, Some(ScreenshotSave::InFlight { .. }))
    }

    #[must_use]
    pub(crate) const fn screenshot_sequence_reserved(&self) -> bool {
        self.screenshot_save_in_flight()
    }
}

#[cfg(test)]
#[path = "screenshot/tests.rs"]
mod snapshot_tests;
