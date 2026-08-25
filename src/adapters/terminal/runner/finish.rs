//! Aggregated runtime cleanup reporting.

use crate::adapters::terminal::TerminalError;

pub(in crate::adapters::terminal) fn runtime(
    run_result: Result<(), TerminalError>,
    cleanup_results: impl IntoIterator<Item = (&'static str, Result<(), TerminalError>)>,
    elapsed: std::time::Duration,
) -> Result<(), TerminalError> {
    let mut failures = cleanup_results
        .into_iter()
        .filter_map(|(stage, result)| result.err().map(|error| (stage, error)))
        .map(|(stage, error)| {
            super::diagnostics::cleanup_failed(stage);
            format!("{stage}: {error}")
        })
        .collect::<Vec<_>>();
    if let Err(error) = run_result {
        super::diagnostics::cleanup_failed("runtime");
        failures.insert(0, format!("runtime: {error}"));
    }
    super::diagnostics::shutdown_finished(failures.len(), elapsed);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(TerminalError::Cleanup(failures.join("; ")))
    }
}
