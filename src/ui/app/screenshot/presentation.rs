//! Canonical Screenshot Inbox command and chrome projections.

use crate::application::ScreenshotPauseReason;

use super::super::BoardApp;
use super::{ScreenshotPaletteAction, ScreenshotState};

impl BoardApp {
    #[must_use]
    pub(in crate::ui::app) const fn screenshot_palette_action(&self) -> ScreenshotPaletteAction {
        match self.screenshot.state {
            ScreenshotState::Off => ScreenshotPaletteAction::Enable,
            ScreenshotState::Paused(_) => ScreenshotPaletteAction::Resume,
            ScreenshotState::Starting | ScreenshotState::Listening => {
                ScreenshotPaletteAction::Disable
            }
            ScreenshotState::Stopping | ScreenshotState::Releasing => {
                ScreenshotPaletteAction::Unavailable
            }
        }
    }

    pub(in crate::ui) fn screenshot_pause_notice(&self) -> Option<&str> {
        self.screenshot.pause_notice.as_deref()
    }

    pub(in crate::ui::app) fn screenshot_footer_state(&self, compact: bool) -> Option<String> {
        match self.screenshot.state {
            ScreenshotState::Listening => Some(if compact {
                "inbox".to_owned()
            } else {
                "inbox listening".to_owned()
            }),
            ScreenshotState::Paused(reason) => Some(if compact {
                "inbox paused".to_owned()
            } else {
                pause_footer(reason)
            }),
            ScreenshotState::Stopping => Some("inbox stopping".to_owned()),
            ScreenshotState::Releasing => Some("inbox releasing".to_owned()),
            ScreenshotState::Off | ScreenshotState::Starting => None,
        }
    }
}

pub(super) fn pause_notice(reason: ScreenshotPauseReason) -> String {
    format!("Screenshot Inbox paused after {}", reason.description())
}

fn pause_footer(reason: ScreenshotPauseReason) -> String {
    match reason {
        ScreenshotPauseReason::Inactivity { .. } => "inbox paused · inactive".to_owned(),
        ScreenshotPauseReason::CaptureLimit { captures } => {
            format!("inbox paused · {captures} captures")
        }
    }
}
