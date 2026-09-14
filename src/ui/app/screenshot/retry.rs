//! Retained capture retry admission across screenshot and durability ownership.

use crate::{
    application::{DurabilityState, Effect},
    ports::environment::{Clock, IdGenerator},
};

use super::{BoardApp, ScreenshotSave};

impl BoardApp {
    pub(in crate::ui::app) fn retry_screenshot_capture(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            self.set_warning("resolve the failed save before retrying the screenshot capture");
            return Vec::new();
        }
        let Some(ScreenshotSave::Ready(candidate)) = self.screenshot.save.clone() else {
            self.set_warning("Screenshot Inbox has no failed capture to retry");
            return Vec::new();
        };
        self.prepare_screenshot_save(candidate, ids, clock, true)
    }
}
