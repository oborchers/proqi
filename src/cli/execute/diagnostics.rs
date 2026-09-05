//! Explicit local diagnostics collection.

use std::path::PathBuf;

use serde_json::json;

use std::path::Path;

use super::Outcome;
use crate::cli::{
    args::{Cli, Command, DiagnosticsCommand},
    output::CliError,
};
use crate::ports::environment::AppPaths;
use crate::{adapters::runtime::SystemEnvironment, ports::environment::Environment as _};

pub(super) fn early_outcome(cli: &Cli) -> Result<Option<Outcome>, CliError> {
    let paths = match &cli.command {
        Some(Command::Diagnostics(_) | Command::Doctor) => {
            super::super::runtime::resolve_paths(cli.state_dir.as_deref())?
        }
        _ => return Ok(None),
    };
    match &cli.command {
        Some(Command::Diagnostics(arguments)) => {
            let cwd = SystemEnvironment
                .current_directory()
                .map_err(|error| CliError::new("environment_failed", error.to_string(), 1))?;
            execute(&paths, &cwd, &arguments.command).map(Some)
        }
        Some(Command::Doctor) => super::doctor::execute(&paths).map(Some),
        _ => Ok(None),
    }
}

pub(super) fn execute(
    paths: &AppPaths,
    cwd: &Path,
    command: &DiagnosticsCommand,
) -> Result<Outcome, CliError> {
    match command {
        DiagnosticsCommand::Collect { output } => collect(paths, cwd, output.clone()),
        DiagnosticsCommand::Keypress => inspect_keypress(paths),
    }
}

fn inspect_keypress(paths: &AppPaths) -> Result<Outcome, CliError> {
    crate::adapters::terminal::require_interactive()?;
    let settings = crate::adapters::terminal::inspect_settings(&paths.config_dir)?;
    let inspection = crate::adapters::terminal::inspect_keypress(&settings.shortcut_registry)?;
    Ok(Outcome {
        data: json!({
            "raw_event": inspection.raw_event,
            "matched_action": inspection.matched_action,
        }),
        human: format!(
            "Raw event: {}\nMatched action: {}",
            inspection.raw_event,
            inspection.matched_action.as_deref().unwrap_or("none")
        ),
    })
}

fn collect(paths: &AppPaths, cwd: &Path, output: Option<PathBuf>) -> Result<Outcome, CliError> {
    let output = output.map_or_else(
        || cwd.join("proqi-diagnostics.json"),
        |path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        },
    );
    let bundle = crate::adapters::diagnostics::collect_bundle(&paths.data_dir, &output)
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
