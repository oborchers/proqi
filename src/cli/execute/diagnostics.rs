//! Explicit local diagnostics collection.

use crate::cli::error_code::ErrorCode;
use std::path::PathBuf;

use serde_json::{Value, json};

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
                .map_err(|error| CliError::new(ErrorCode::EnvironmentFailed, error.to_string()))?;
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
        DiagnosticsCommand::Keypress {
            context,
            timeout_ms,
            defaults,
        } => inspect_keypress(paths, context, *timeout_ms, *defaults),
    }
}

fn inspect_keypress(
    paths: &AppPaths,
    names: &[String],
    timeout_ms: u64,
    defaults: bool,
) -> Result<Outcome, CliError> {
    use crate::ui::{ShortcutContext, ShortcutContextStack, ShortcutRegistry};
    let contexts = names
        .iter()
        .map(|name| {
            ShortcutContext::parse_configuration_id(name).ok_or_else(|| {
                CliError::new(
                    ErrorCode::InvalidShortcutContext,
                    "unknown context; use a documented keymap context identifier".to_owned(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if contexts.is_empty() || contexts.len() > 21 {
        return Err(CliError::new(
            ErrorCode::InvalidShortcutContext,
            "provide between 1 and 21 contexts, bottom to top".to_owned(),
        ));
    }
    let contexts = ShortcutContextStack::new(contexts);
    let registry = if defaults {
        ShortcutRegistry::default()
    } else {
        crate::adapters::terminal::inspect_settings(&paths.config_dir)?
            .ui
            .shortcuts
    };
    crate::adapters::terminal::require_interactive()?;
    let inspection = crate::adapters::terminal::inspect_keypress(
        &registry,
        &contexts,
        std::time::Duration::from_millis(timeout_ms),
    )?;
    let event = inspection.event;
    let status = if inspection.cancelled {
        "cancelled"
    } else {
        event.as_ref().map_or("no_event_received", |event| {
            if event.capture_cancelled() {
                "cancelled"
            } else {
                "event_received"
            }
        })
    };
    let explanation = if inspection.cancelled {
        "Capture cancelled by a termination request."
    } else if event.is_none() {
        "No key event received. Proqi cannot know whether Ghostty, the OS, Karabiner, Herdr, or another layer consumed it."
    } else {
        "Captured one logical event. Resolution uses the selected context stack; no application action was executed."
    };
    let data = json!({
        "capture_schema_version": 1,
        "status": status,
        "event": event.as_ref().map(inspection_value),
        "context_source": "diagnostic_selection",
        "keymap_source": if defaults { "defaults" } else { "configuration" },
        "context_stack": contexts
            .as_slice()
            .iter()
            .map(|context| context.configuration_id())
            .collect::<Vec<_>>(),
        "timeout_ms": timeout_ms,
        "explanation": explanation,
    });
    Ok(Outcome {
        human: format!("{explanation}\n{data}"),
        data,
    })
}

fn inspection_value(inspection: &crate::ui::ShortcutInspection) -> Value {
    let action = inspection
        .action
        .map(crate::ui::ShortcutActionId::diagnostics_id);
    let stroke: crate::ui::ShortcutStrokeInspection = inspection.stroke_inspection();
    json!({
        "capture_cancelled": inspection.capture_cancelled(),
        "keystroke": {
            "key": stroke.key,
            "modifiers": stroke.modifiers,
            "state": stroke.state,
            "phase": stroke.phase,
        },
        "platform": inspection.platform_id(),
        "primary": inspection.primary_names(),
        "context_stack": inspection
            .context_stack
            .iter()
            .map(|context| context.configuration_id())
            .collect::<Vec<_>>(),
        "active_context": inspection
            .active_context
            .map(crate::ui::ShortcutContext::configuration_id),
        "classification": inspection.classification_id(),
        "action": action,
        "ui_intention": inspection.intention_name(),
        "binding_identity": inspection.binding_identity(),
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
        .map_err(|error| CliError::new(ErrorCode::DiagnosticsFailed, error.to_string()))?;
    Ok(Outcome {
        data: json!({
            "path": output,
            "bundle_schema_version": bundle.schema_version,
            "files": bundle.files.len(),
        }),
        human: format!("Diagnostics written to {}", output.display()),
    })
}
