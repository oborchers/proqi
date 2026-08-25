//! Explicit local diagnostics collection.

use std::path::PathBuf;

use serde_json::json;

use super::{Outcome, RuntimeContext};
use crate::cli::{args::DiagnosticsCommand, output::CliError};

pub(super) fn execute(
    context: &RuntimeContext,
    command: DiagnosticsCommand,
) -> Result<Outcome, CliError> {
    match command {
        DiagnosticsCommand::Collect { output } => collect(context, output),
    }
}

fn collect(context: &RuntimeContext, output: Option<PathBuf>) -> Result<Outcome, CliError> {
    let output = output.map_or_else(
        || context.cwd.join("proqi-diagnostics.json"),
        |path| {
            if path.is_absolute() {
                path
            } else {
                context.cwd.join(path)
            }
        },
    );
    let bundle = crate::adapters::diagnostics::collect_bundle(&context.data_dir, &output)
        .map_err(|error| CliError::new("diagnostics_failed", error.to_string(), 1))?;
    Ok(Outcome {
        data: json!({
            "path": output,
            "bundle_schema_version": bundle.schema_version,
            "files": bundle.files.len(),
        }),
        human: format!("Diagnostics written to {}", output.display()),
    })
}
