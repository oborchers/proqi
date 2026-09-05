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

#[cfg(test)]
mod tests {
    use crate::{
        adapters::{diagnostics::SafeEvent, memory::FakeIdGenerator},
        ports::{
            agent::SubmissionRouteKind, environment::IdGenerator as _,
            store::SubmissionJournalRoute,
        },
    };

    #[test]
    fn global_submission_diagnostic_projection_has_no_content_or_topology_fields() {
        let route = SubmissionJournalRoute::herdr_agent();
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let event = SafeEvent::Submission {
            submission_id: ids.submission_id(),
            state: "preparing",
            route_kind: route.kind(),
            direction: route.adjacent_direction(),
            provider: super::provider_name("/secret/provider/w2:p9"),
            outcome: None,
        };
        let diagnostic = format!("{event:?}");

        assert!(diagnostic.contains("HerdrAgent"));
        assert!(diagnostic.contains("provider: \"unknown\""));
        assert!(!diagnostic.contains("secret"));
        assert!(!diagnostic.contains("w2:p9"));
        assert!(!diagnostic.contains("prompt body"));
        assert_eq!(route.kind(), SubmissionRouteKind::HerdrAgent);
        assert_eq!(route.adjacent_direction(), None);
    }
}
