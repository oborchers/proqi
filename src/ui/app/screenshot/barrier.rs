//! Lossless bounded input admission while one commit-first capture owns the sequence boundary.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
    ui::{PointerInput, PointerKind, UiInput},
};

use super::super::BoardApp;

const DEFERRED_INPUT_LIMIT: usize = 64;

pub(super) struct DeferredInput {
    pub(super) input: UiInput,
    pub(super) received_at: crate::domain::Timestamp,
    replay_layout: Option<Box<crate::ui::LayoutSnapshot>>,
}

struct ReceiptClock(crate::domain::Timestamp);

impl Clock for ReceiptClock {
    fn now(&self) -> crate::domain::Timestamp {
        self.0
    }
}

impl BoardApp {
    pub(crate) fn handle_screenshot_commit_barrier(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::HostFocusGained => return Self::discover_agents(),
            input @ (UiInput::Pointer(PointerInput {
                kind: PointerKind::Move,
                ..
            }) | UiInput::Resize { .. }) => return self.handle_primary_input(input, ids, clock),
            deferred @ (UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_))
                if self.deferred_deliberate_count() < DEFERRED_INPUT_LIMIT =>
            {
                self.screenshot.deferred_inputs.push_back(DeferredInput {
                    replay_layout: matches!(deferred, UiInput::Pointer(_))
                        .then(|| self.layout.clone().map(Box::new))
                        .flatten(),
                    input: deferred,
                    received_at: clock.now(),
                });
            }
            UiInput::Pointer(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_) => self.set_error(
                "Screenshot Inbox input queue is full; that input was not accepted—wait for the save result and retry",
            ),
        }
        Vec::new()
    }

    pub(super) fn replay_screenshot_inputs(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        let queued = std::mem::take(&mut self.screenshot.deferred_inputs);
        let mut effects = Vec::new();
        for deferred in queued {
            if matches!(deferred.input, UiInput::Pointer(_)) {
                self.layout = deferred.replay_layout.map(|layout| *layout);
            }
            effects.extend(self.handle(deferred.input, ids, &ReceiptClock(deferred.received_at)));
            if self.quit {
                break;
            }
        }
        effects
    }

    pub(crate) fn screenshot_barrier_accepts(&self, input: &UiInput) -> bool {
        !self.screenshot_save_in_flight()
            || matches!(
                input,
                UiInput::HostFocusGained
                    | UiInput::Resize { .. }
                    | UiInput::Pointer(PointerInput {
                        kind: PointerKind::Move,
                        ..
                    })
            )
            || self.deferred_deliberate_count() < DEFERRED_INPUT_LIMIT
    }

    pub(super) fn deferred_deliberate_count(&self) -> usize {
        self.screenshot
            .deferred_inputs
            .iter()
            .filter(|deferred| deferred.input.is_deliberate_interaction())
            .count()
    }
}
