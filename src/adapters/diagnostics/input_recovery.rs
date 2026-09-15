//! Content-free input recovery diagnostic projection.

pub(super) fn record(
    stage: &str,
    reason: Option<&str>,
    attempt_count: usize,
    outcome: Option<&str>,
) {
    tracing::info!(
        event = "input_recovery",
        stage,
        reason,
        attempt_count,
        outcome
    );
}
