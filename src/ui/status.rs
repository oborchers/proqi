//! Responsive transient status semantics for the stable footer row.

use super::BoardApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum StatusSeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusOwner {
    General,
    AttachmentRefresh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct UiStatus {
    message: String,
    severity: StatusSeverity,
    owner: StatusOwner,
}

impl UiStatus {
    fn new(message: impl Into<String>, severity: StatusSeverity) -> Self {
        Self {
            message: message.into(),
            severity,
            owner: StatusOwner::General,
        }
    }

    fn attachment(message: impl Into<String>, severity: StatusSeverity) -> Self {
        Self {
            message: message.into(),
            severity,
            owner: StatusOwner::AttachmentRefresh,
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

    pub(crate) fn set_attachment_info(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::attachment(message, StatusSeverity::Info));
    }

    pub(crate) fn set_attachment_success(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::attachment(message, StatusSeverity::Success));
    }

    pub(crate) fn set_attachment_warning(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::attachment(message, StatusSeverity::Warning));
    }

    pub(crate) fn clear_attachment_status(&mut self) {
        if self
            .status
            .as_ref()
            .is_some_and(|status| status.owner == StatusOwner::AttachmentRefresh)
        {
            self.status = None;
        }
    }

    pub(crate) fn clear_status_for_interaction(&mut self) {
        let active_refresh = self.state.attachments.manual_refresh_active()
            && self
                .status
                .as_ref()
                .is_some_and(|status| status.owner == StatusOwner::AttachmentRefresh);
        if !active_refresh {
            self.status = None;
        }
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
