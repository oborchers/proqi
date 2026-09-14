//! Screenshot state consulted before update mutation admission closes.

use super::{BoardApp, ScreenshotSave, ScreenshotState};

/// Whether update preparation may close ordinary mutation admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotUpdateReadiness {
    /// No watcher or accepted capture can still produce a durable mutation.
    Ready,
    /// One already accepted capture is finishing its commit-first save.
    CommitInFlight,
    /// Watcher activity or retained undurable capture state requires explicit resolution.
    Blocked,
}

impl BoardApp {
    pub(crate) fn screenshot_update_readiness(&self) -> ScreenshotUpdateReadiness {
        match &self.screenshot.save {
            Some(ScreenshotSave::InFlight { .. }) => ScreenshotUpdateReadiness::CommitInFlight,
            None if self.screenshot.candidates.is_empty()
                && matches!(
                    self.screenshot.state,
                    ScreenshotState::Off | ScreenshotState::Paused(_)
                ) =>
            {
                ScreenshotUpdateReadiness::Ready
            }
            Some(ScreenshotSave::Ready(_)) | None => ScreenshotUpdateReadiness::Blocked,
        }
    }
}
