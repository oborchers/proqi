//! Responsive transient status semantics for the stable footer row.

use super::BoardApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum StatusSeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct UiStatus {
    message: String,
    severity: StatusSeverity,
}

impl UiStatus {
    fn new(message: impl Into<String>, severity: StatusSeverity) -> Self {
        Self {
            message: message.into(),
            severity,
        }
    }

    pub(in crate::ui) fn view(&self) -> (&str, StatusSeverity) {
        (&self.message, self.severity)
    }
}

impl BoardApp {
    pub(crate) fn set_info(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::new(message, StatusSeverity::Info));
    }

    pub(crate) fn set_success(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::new(message, StatusSeverity::Success));
    }

    pub(crate) fn set_warning(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::new(message, StatusSeverity::Warning));
    }

    pub(crate) fn set_error(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::new(message, StatusSeverity::Error));
    }

    /// Current transient status text for accessibility and contract tests.
    #[must_use]
    pub fn status_text(&self) -> Option<&str> {
        self.status
            .as_ref()
            .map(|status| status.message.as_str())
            .or_else(|| self.screenshot_pause_notice())
    }

    pub(in crate::ui) fn status_view(&self) -> Option<(&str, StatusSeverity)> {
        self.status.as_ref().map(UiStatus::view).or_else(|| {
            self.screenshot_pause_notice()
                .map(|notice| (notice, StatusSeverity::Warning))
        })
    }
}
