//! Stable, content-free phase timing for local and hosted diagnostics.

use std::time::Instant;

use serde_json::json;

pub(super) fn phase<T>(
    name: &str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let started = Instant::now();
    let result = operation();
    println!(
        "proqi-timing {}",
        json!({
            "schema_version": 1,
            "phase": name,
            "elapsed_ms": started.elapsed().as_millis(),
            "outcome": if result.is_ok() { "success" } else { "failure" },
        })
    );
    result
}

#[cfg(test)]
mod tests {
    use super::phase;

    #[test]
    fn timing_preserves_success_and_failure() {
        assert_eq!(phase("test.success", || Ok(7)), Ok(7));
        assert_eq!(
            phase::<()>("test.failure", || Err("failed".to_owned())),
            Err("failed".to_owned())
        );
    }
}
