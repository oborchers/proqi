use crate::{
    application::{DurabilityState, FailureCode},
    domain::OperationSequence,
};

use super::Applicability;

#[derive(Clone, Copy)]
pub(in crate::ui::app::palette) struct RecoveryContext {
    failed: bool,
    retry_available: bool,
    quit_available: bool,
}

impl RecoveryContext {
    pub(super) fn capture(
        durability: &DurabilityState,
        exported_for: Option<OperationSequence>,
    ) -> Self {
        match *durability {
            DurabilityState::Failed { failed, code, .. } => Self {
                failed: true,
                retry_available: !matches!(code, FailureCode::RecoveryCapacity),
                quit_available: exported_for == Some(failed),
            },
            DurabilityState::Durable { .. } | DurabilityState::Pending { .. } => Self {
                failed: false,
                retry_available: false,
                quit_available: true,
            },
        }
    }

    pub(super) const fn failed(self) -> bool {
        self.failed
    }

    pub(super) const fn retry_applicability(self) -> Applicability {
        if self.failed {
            when(
                self.retry_available,
                "Retry unavailable; export recovery instead",
            )
        } else {
            Applicability::disabled("Available after a save failure")
        }
    }

    pub(super) const fn export_applicability(self) -> Applicability {
        when(self.failed, "Available after a save failure")
    }

    pub(super) const fn quit_applicability(self) -> Applicability {
        when(self.quit_available, "Export recovery before quitting")
    }
}

const fn when(condition: bool, reason: &'static str) -> Applicability {
    if condition {
        Applicability::ENABLED
    } else {
        Applicability::disabled(reason)
    }
}
