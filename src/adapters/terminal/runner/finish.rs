//! Aggregated runtime cleanup reporting.

use crate::adapters::terminal::TerminalError;

pub(in crate::adapters::terminal) fn runtime(
    run_result: Result<(), TerminalError>,
    cleanup_results: impl IntoIterator<Item = Result<(), TerminalError>>,
) -> Result<(), TerminalError> {
    let mut failures = cleanup_results
        .into_iter()
        .filter_map(Result::err)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if let Err(error) = run_result {
        failures.insert(0, error.to_string());
    }
    super::diagnostics::shutdown_finished(failures.len());
    if failures.is_empty() {
        Ok(())
    } else {
        Err(TerminalError::Cleanup(failures.join("; ")))
    }
}
