//! Named session creation and get-or-create without launching a TUI.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde_json::json;

use crate::{application::NamedSession, ports::runtime::RuntimeCoordinator};

use super::{
    super::helpers::parse_operation_id, CliError, Outcome, RuntimeContext,
    listing::availability_state, resume_command, session_service,
};

pub(super) fn ensure(
    context: &mut RuntimeContext,
    name: String,
    cwd: &Path,
) -> Result<Outcome, CliError> {
    let cwd = existing_directory(cwd)?;
    let result = session_service(context)?.ensure_named_session(name, cwd)?;
    outcome(context, &result)
}

pub(super) fn create(
    context: &mut RuntimeContext,
    name: String,
    cwd: Option<&Path>,
    operation: Option<&str>,
) -> Result<Outcome, CliError> {
    let operation = parse_operation_id(operation)?;
    let cwd = existing_directory(cwd.unwrap_or(&context.cwd))?;
    let result = session_service(context)?.create_named_session(name, cwd, operation)?;
    outcome(context, &result)
}

/// Resolve a caller-supplied origin exactly like an interactive launch directory.
pub(in crate::cli::execute) fn existing_directory(path: &Path) -> Result<PathBuf, CliError> {
    crate::adapters::filesystem::canonical_existing_directory(path).map_err(|error| {
        CliError::input(format!(
            "session directory must be an existing directory: {}: {error}",
            path.display()
        ))
    })
}

fn outcome(context: &mut RuntimeContext, result: &NamedSession) -> Result<Outcome, CliError> {
    let runtime = context.coordinator.scan_runtime()?;
    let active: HashSet<_> = runtime
        .active
        .into_iter()
        .map(|instance| instance.session_id)
        .collect();
    let recovered: HashSet<_> = runtime.recovered.into_iter().collect();
    let session = session_service(context)?
        .inspect_session(result.session_id)?
        .board
        .session;
    let state = availability_state(
        session.id,
        session.deleted_at.is_some(),
        &active,
        &recovered,
    );
    let resume = resume_command(session.id);
    let disposition = result.disposition.as_str();
    let mut data = json!({
        "session_id": session.id,
        "name": session.name,
        "origin_cwd": session.origin_cwd,
        "state": state,
        "disposition": disposition,
        "resume_command": resume,
    });
    let mut human = format!(
        "Session {} {disposition}\nResume later: {resume}",
        session.id
    );
    if let Some(operation_id) = result.operation_id {
        data["receipt"] = json!({
            "session_id": session.id,
            "operation_id": operation_id,
            "idempotent_replay": result.idempotent_replay,
        });
        if result.idempotent_replay {
            human.push_str("\n(replay)");
        }
    }
    Ok(Outcome { data, human })
}
