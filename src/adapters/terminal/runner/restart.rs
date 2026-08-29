//! Verified post-cleanup process replacement for an accepted update.

use crate::{
    adapters::terminal::TerminalError,
    domain::{Installation, InstallationKind, SessionId, StableVersion},
};

pub(super) fn resume_after_update(
    installation: Option<&Installation>,
    requested: Option<&StableVersion>,
    session_id: SessionId,
    state_root: Option<&std::path::Path>,
) -> Result<SessionId, TerminalError> {
    requested.map_or(Ok(session_id), |version| {
        replace_after_cleanup(installation, version, session_id, state_root)
    })
}

fn replace_after_cleanup(
    installation: Option<&Installation>,
    expected: &StableVersion,
    session_id: SessionId,
    state_root: Option<&std::path::Path>,
) -> Result<SessionId, TerminalError> {
    use crate::ports::update::{InstallDetector as _, ProcessReplacer as _};

    let installation = installation.ok_or_else(|| {
        TerminalError::Io("update restart lacks a verified installation context".to_owned())
    })?;
    if installation.kind != InstallationKind::HomebrewFormula {
        return Err(TerminalError::Io(
            "automatic restart is available only for verified Homebrew installations".to_owned(),
        ));
    }
    let active = installation
        .restart_executable
        .as_ref()
        .ok_or_else(|| TerminalError::Io("Homebrew active executable is unavailable".to_owned()))?;
    let detected = crate::adapters::update::SystemInstallDetector::for_executable(active.clone())
        .detect()
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    if detected.kind != InstallationKind::HomebrewFormula
        || detected.identity != installation.identity
    {
        return Err(TerminalError::Io(
            "updated executable does not belong to this Homebrew installation".to_owned(),
        ));
    }
    let mut runner = crate::adapters::process::SystemProcessRunner::default();
    crate::adapters::update::verify_installed_version(&mut runner, &detected.executable, expected)
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    crate::adapters::process::SystemProcessReplacer
        .replace(&detected.executable, session_id, state_root)
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    Ok(session_id)
}
