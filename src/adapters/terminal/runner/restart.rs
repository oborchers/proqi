//! Verified post-cleanup process replacement for an accepted update.

use crate::{
    adapters::terminal::TerminalError,
    domain::{Installation, InstallationKind, SessionId, StableVersion},
};

pub(super) fn resume_after_update(
    installation: Option<&Installation>,
    requested: Option<&(StableVersion, crate::domain::RequestId)>,
    session_id: SessionId,
    state_root: Option<&std::path::Path>,
    previous_instance_id: crate::domain::InstanceId,
) -> Result<SessionId, TerminalError> {
    requested.map_or(Ok(session_id), |(version, operation_id)| {
        replace_after_cleanup(
            installation,
            version,
            session_id,
            state_root,
            *operation_id,
            previous_instance_id,
        )
    })
}

fn replace_after_cleanup(
    installation: Option<&Installation>,
    expected: &StableVersion,
    session_id: SessionId,
    state_root: Option<&std::path::Path>,
    operation_id: crate::domain::RequestId,
    previous_instance_id: crate::domain::InstanceId,
) -> Result<SessionId, TerminalError> {
    use crate::ports::update::{InstallDetector as _, ProcessReplacer as _};

    let recovery = || {
        format!(
            "resume exact session {session_id} with the active Proqi executable and the same state root"
        )
    };
    let installation = installation.ok_or_else(|| {
        TerminalError::Io(format!(
            "update restart lacks a verified installation context; {}",
            recovery()
        ))
    })?;
    if installation.kind != InstallationKind::HomebrewFormula {
        return Err(TerminalError::Io(format!(
            "automatic restart is available only for verified Homebrew installations; {}",
            recovery()
        )));
    }
    let active = installation.restart_executable.as_ref().ok_or_else(|| {
        TerminalError::Io(format!(
            "Homebrew active executable is unavailable; {}",
            recovery()
        ))
    })?;
    let detected = crate::adapters::update::SystemInstallDetector::for_executable(active.clone())
        .detect()
        .map_err(|error| TerminalError::Io(format!("{error}; {}", recovery())))?;
    if detected.kind != InstallationKind::HomebrewFormula
        || detected.identity != installation.identity
    {
        return Err(TerminalError::Io(format!(
            "updated executable does not belong to this Homebrew installation; {}",
            recovery()
        )));
    }
    let mut runner = crate::adapters::process::SystemProcessRunner::default();
    crate::adapters::update::verify_installed_version(&mut runner, &detected.executable, expected)
        .map_err(|error| TerminalError::Io(format!("{error}; {}", recovery())))?;
    crate::adapters::process::SystemProcessReplacer
        .replace(
            &detected.executable,
            session_id,
            state_root,
            operation_id,
            previous_instance_id,
            expected,
        )
        .map_err(|error| TerminalError::Io(format!("{error}; {}", recovery())))?;
    Ok(session_id)
}
