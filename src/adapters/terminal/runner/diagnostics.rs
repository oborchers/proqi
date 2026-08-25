//! Typed runtime lifecycle diagnostics.

use crate::adapters::diagnostics::{SafeEvent, record};

pub(super) fn begin_shutdown(owned: &mut super::OwnedLanes) {
    record(SafeEvent::ShutdownStarted);
    owned.request_stop();
}

pub(super) fn cleanup_failed(stage: &'static str) {
    record(SafeEvent::CleanupFailed { stage });
}

pub(super) fn shutdown_finished(cleanup_failures: usize, elapsed: std::time::Duration) {
    record(SafeEvent::ShutdownFinished {
        cleanup_failures,
        elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
    });
}

pub(super) fn provider_name(value: &str) -> &'static str {
    if value == "herdr" { "herdr" } else { "unknown" }
}
