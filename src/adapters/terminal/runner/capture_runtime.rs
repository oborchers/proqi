//! Mutable screenshot capture ownership facts shared by runner consumers.

use std::time::Instant;

use crate::adapters::{control::ControlDeliveryReceipt, runtime::FileCaptureLease};

#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "orthogonal watcher, requester, takeover, and release facts have independent transitions"
)]
pub(super) struct CaptureRuntime {
    pub(super) lease: Option<FileCaptureLease>,
    pub(super) release_when_drained: bool,
    pub(super) shutdown_requested: bool,
    pub(super) takeover_delivery: Option<ControlDeliveryReceipt>,
    pub(super) takeover_stopping: bool,
    pub(super) watcher_stopped: bool,
    pub(super) release_deadline: Option<Instant>,
}
