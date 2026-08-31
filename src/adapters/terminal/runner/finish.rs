//! Aggregated runtime cleanup reporting.

use crate::adapters::terminal::TerminalError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::adapters::terminal) enum CleanupStage {
    Accessibility,
    Input,
    Persistence,
    External,
    Update,
    Screenshot,
    Control,
    TerminalRestoration,
    Runtime,
}

impl CleanupStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::Input => "input",
            Self::Persistence => "persistence",
            Self::External => "external",
            Self::Update => "update",
            Self::Screenshot => "screenshot",
            Self::Control => "control",
            Self::TerminalRestoration => "terminal_restoration",
            Self::Runtime => "runtime",
        }
    }
}

pub(in crate::adapters::terminal) fn runtime(
    run_result: Result<(), TerminalError>,
    cleanup_results: impl IntoIterator<Item = (CleanupStage, Result<(), TerminalError>)>,
    elapsed: std::time::Duration,
) -> Result<(), TerminalError> {
    let mut failures = cleanup_results
        .into_iter()
        .filter_map(|(stage, result)| result.err().map(|error| (stage, error)))
        .map(|(stage, error)| {
            let stage = stage.as_str();
            super::diagnostics::cleanup_failed(stage);
            format!("{stage}: {error}")
        })
        .collect::<Vec<_>>();
    if let Err(error) = run_result {
        let stage = CleanupStage::Runtime.as_str();
        super::diagnostics::cleanup_failed(stage);
        failures.insert(0, format!("{stage}: {error}"));
    }
    super::diagnostics::shutdown_finished(failures.len(), elapsed);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(TerminalError::Cleanup(failures.join("; ")))
    }
}
