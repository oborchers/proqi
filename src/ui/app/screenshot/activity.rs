//! Deterministic unattended-capture lease accounting.

use std::time::Duration;

use crate::{
    application::ScreenshotPauseReason,
    ports::screenshot::ScreenshotActivityPolicy,
    ui::{PointerKind, UiInput},
};

#[derive(Default)]
pub(super) struct ScreenshotActivity {
    policy: ScreenshotActivityPolicy,
    last_interaction: Option<Duration>,
    admitted: u16,
}

impl ScreenshotActivity {
    pub(super) const fn configure(&mut self, policy: ScreenshotActivityPolicy) {
        self.policy = policy;
    }

    pub(super) const fn start(&mut self, now: Duration) {
        self.last_interaction = Some(now);
        self.admitted = 0;
    }

    pub(super) const fn stop(&mut self) {
        self.last_interaction = None;
        self.admitted = 0;
    }

    pub(super) fn note_input(&mut self, input: &UiInput, now: Duration) {
        if self.last_interaction.is_some() && deliberate(input) {
            self.last_interaction = Some(now);
            self.admitted = 0;
        }
    }

    pub(super) fn expired(&self, now: Duration) -> Option<ScreenshotPauseReason> {
        let last = self.last_interaction?;
        (now.saturating_sub(last) >= self.policy.inactivity_timeout()).then_some(
            ScreenshotPauseReason::Inactivity {
                minutes: self.policy.inactivity_timeout_minutes(),
            },
        )
    }

    pub(super) fn remaining(&self) -> usize {
        usize::from(
            self.policy
                .max_unattended_captures()
                .saturating_sub(self.admitted),
        )
    }

    pub(super) fn admit(&mut self, count: usize) -> Option<ScreenshotPauseReason> {
        let bounded = u16::try_from(count).unwrap_or(u16::MAX);
        self.admitted = self.admitted.saturating_add(bounded);
        (self.admitted >= self.policy.max_unattended_captures()).then_some(
            ScreenshotPauseReason::CaptureLimit {
                captures: self.policy.max_unattended_captures(),
            },
        )
    }
}

const fn deliberate(input: &UiInput) -> bool {
    match input {
        UiInput::Key(_) | UiInput::Paste(_) | UiInput::PasteAnnotated(_) => true,
        UiInput::Pointer(pointer) => matches!(
            pointer.kind,
            PointerKind::Down(_)
                | PointerKind::Drag(_)
                | PointerKind::ScrollUp
                | PointerKind::ScrollDown
        ),
        UiInput::Resize { .. } | UiInput::HostFocusGained => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{PointerButton, PointerInput, UiKey};

    #[test]
    fn only_deliberate_input_renews_the_lease() {
        let mut lease = ScreenshotActivity::default();
        lease.start(Duration::ZERO);
        let almost = Duration::from_secs(20 * 60 - 1);
        lease.note_input(&UiInput::HostFocusGained, almost);
        assert!(lease.expired(Duration::from_secs(20 * 60)).is_some());

        lease.start(Duration::ZERO);
        lease.note_input(&UiInput::Key(UiKey::Character('x')), almost);
        assert!(lease.expired(Duration::from_secs(20 * 60)).is_none());
        lease.note_input(
            &UiInput::Pointer(PointerInput {
                column: 1,
                row: 1,
                kind: PointerKind::Down(PointerButton::Left),
                extend_selection: false,
            }),
            Duration::from_secs(20 * 60),
        );
        assert!(lease.expired(Duration::from_secs(30 * 60)).is_none());
    }
}
