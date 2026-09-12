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
    ScreenshotAutoPause,
    ScreenshotFailure,
    StorageFailure,
    Recovery,
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

    fn screenshot_auto_pause(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            severity: StatusSeverity::Warning,
            owner: StatusOwner::ScreenshotAutoPause,
        }
    }

    fn owned(message: impl Into<String>, severity: StatusSeverity, owner: StatusOwner) -> Self {
        Self {
            message: message.into(),
            severity,
            owner,
        }
    }

    pub(in crate::ui) fn view(&self) -> (&str, StatusSeverity) {
        (&self.message, self.severity)
    }
}

impl BoardApp {
    pub(crate) fn set_info(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::new(message, StatusSeverity::Info));
        }
    }

    pub(crate) fn set_success(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::new(message, StatusSeverity::Success));
        }
    }

    pub(crate) fn set_warning(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::new(message, StatusSeverity::Warning));
        }
    }

    pub(crate) fn set_error(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::new(message, StatusSeverity::Error));
        }
    }

    pub(crate) fn set_attachment_info(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::attachment(message, StatusSeverity::Info));
        }
    }

    pub(crate) fn set_attachment_success(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::attachment(message, StatusSeverity::Success));
        }
    }

    pub(crate) fn set_attachment_warning(&mut self, message: impl Into<String>) {
        if !self.protected_status_active() {
            self.status = Some(UiStatus::attachment(message, StatusSeverity::Warning));
        }
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

    pub(crate) fn set_screenshot_auto_pause_warning(&mut self, message: impl Into<String>) {
        let protected_status = self
            .status
            .as_ref()
            .is_some_and(|status| status.severity == StatusSeverity::Error);
        if !protected_status
            && !matches!(
                self.state.durability,
                crate::application::DurabilityState::Failed { .. }
            )
        {
            self.status = Some(UiStatus::screenshot_auto_pause(message));
        }
    }

    pub(crate) fn set_screenshot_failure(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::owned(
            message,
            StatusSeverity::Error,
            StatusOwner::ScreenshotFailure,
        ));
    }

    pub(crate) fn set_screenshot_progress(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::owned(
            message,
            StatusSeverity::Info,
            StatusOwner::ScreenshotFailure,
        ));
    }

    pub(crate) fn set_storage_failure(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::owned(
            message,
            StatusSeverity::Error,
            StatusOwner::StorageFailure,
        ));
    }

    pub(crate) fn set_recovery_info(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::owned(
            message,
            StatusSeverity::Info,
            StatusOwner::Recovery,
        ));
    }

    pub(crate) fn set_recovery_success(&mut self, message: impl Into<String>) {
        self.status = Some(UiStatus::owned(
            message,
            StatusSeverity::Success,
            StatusOwner::Recovery,
        ));
    }

    pub(crate) fn clear_screenshot_auto_pause_status(&mut self) {
        if self.screenshot_auto_pause_status_active() {
            self.status = None;
        }
    }

    pub(crate) fn clear_screenshot_failure_status(&mut self) {
        if self.status_owner_is(StatusOwner::ScreenshotFailure) {
            self.status = None;
        }
    }

    pub(crate) fn clear_storage_failure_status(&mut self) {
        if self.status.as_ref().is_some_and(|status| {
            matches!(
                status.owner,
                StatusOwner::StorageFailure | StatusOwner::Recovery
            )
        }) {
            self.status = None;
        }
    }

    pub(crate) fn enter_storage_failure_state(&mut self) {
        if !self.status_owner_is(StatusOwner::StorageFailure) {
            self.status = None;
        }
    }

    pub(crate) fn clear_status_for_interaction(
        &mut self,
        acknowledges_screenshot_auto_pause: bool,
    ) {
        let active_refresh = self.state.attachments.manual_refresh_active()
            && self
                .status
                .as_ref()
                .is_some_and(|status| status.owner == StatusOwner::AttachmentRefresh);
        let protected_auto_pause =
            self.screenshot_auto_pause_status_active() && !acknowledges_screenshot_auto_pause;
        let protected_failure = self.status.as_ref().is_some_and(|status| {
            matches!(
                status.owner,
                StatusOwner::ScreenshotFailure | StatusOwner::StorageFailure
            )
        });
        if !active_refresh && !protected_auto_pause && !protected_failure {
            self.status = None;
        }
    }

    fn screenshot_auto_pause_status_active(&self) -> bool {
        self.status_owner_is(StatusOwner::ScreenshotAutoPause)
    }

    fn status_owner_is(&self, owner: StatusOwner) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.owner == owner)
    }

    fn protected_status_active(&self) -> bool {
        matches!(
            self.status.as_ref().map(|status| status.owner),
            Some(
                StatusOwner::ScreenshotAutoPause
                    | StatusOwner::ScreenshotFailure
                    | StatusOwner::StorageFailure
                    | StatusOwner::Recovery
            )
        ) || matches!(
            self.state.durability,
            crate::application::DurabilityState::Failed { .. }
        )
    }

    /// Current transient status text for accessibility and contract tests.
    #[must_use]
    pub fn status_text(&self) -> Option<&str> {
        self.status.as_ref().map(|status| status.message.as_str())
    }

    pub(in crate::ui) fn status_view(&self) -> Option<(&str, StatusSeverity)> {
        self.status.as_ref().map(UiStatus::view)
    }
}
