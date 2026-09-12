//! In-place refresh of asynchronous Commands capabilities.

use super::BoardApp;

impl BoardApp {
    pub(in crate::ui::app) fn refresh_screenshot_palette_action(&mut self) {
        let screenshot = self.capture_screenshot_command_context();
        if let Some(palette) = &mut self.palette {
            palette.refresh_context(|context| context.set_screenshot(screenshot));
            self.layout = None;
        }
    }

    pub(in crate::ui::app) fn refresh_palette_submission(&mut self) {
        let supported = self.supports_submission();
        if let Some(palette) = &mut self.palette {
            palette.refresh_context(|context| context.set_submit_supported(supported));
            self.layout = None;
        }
    }

    pub(in crate::ui::app) fn refresh_palette_recovery(&mut self) {
        let recovery = self.capture_recovery_command_context();
        if let Some(palette) = &mut self.palette {
            palette.refresh_context(|context| context.set_recovery(recovery));
            self.layout = None;
        }
    }
}
