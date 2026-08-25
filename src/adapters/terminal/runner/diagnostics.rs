//! Typed runtime lifecycle diagnostics.

use crate::adapters::diagnostics::{SafeEvent, record};

pub(super) fn begin_shutdown(owned: &mut super::OwnedLanes) {
    record(SafeEvent::ShutdownStarted);
    owned.request_stop();
}

pub(super) fn shutdown_finished(cleanup_failures: usize) {
    record(SafeEvent::ShutdownFinished { cleanup_failures });
}

pub(super) fn provider_name(value: &str) -> &'static str {
    if value == "herdr" { "herdr" } else { "unknown" }
}
