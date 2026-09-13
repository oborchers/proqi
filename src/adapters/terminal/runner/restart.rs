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
    validate_restart_kind(installation.kind)?;
    let active = installation
        .restart_executable
        .as_ref()
        .ok_or_else(|| TerminalError::Io("active update executable is unavailable".to_owned()))?;
    let detected = crate::adapters::update::SystemInstallDetector::for_executable(active.clone())
        .detect()
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    validate_redetected_installation(installation, &detected)?;
    let mut runner = crate::adapters::process::SystemProcessRunner::default();
    crate::adapters::update::verify_installed_version(&mut runner, &detected.executable, expected)
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    crate::adapters::process::SystemProcessReplacer
        .replace(&detected.executable, session_id, state_root)
        .map_err(|error| TerminalError::Io(error.to_string()))?;
    Ok(session_id)
}

fn validate_restart_kind(kind: InstallationKind) -> Result<(), TerminalError> {
    matches!(
        kind,
        InstallationKind::HomebrewFormula | InstallationKind::StandaloneArchive
    )
    .then_some(())
    .ok_or_else(|| {
        TerminalError::Io("automatic restart requires a verified updatable installation".to_owned())
    })
}

fn validate_redetected_installation(
    expected: &Installation,
    detected: &Installation,
) -> Result<(), TerminalError> {
    (detected.kind == expected.kind && detected.identity == expected.identity)
        .then_some(())
        .ok_or_else(|| {
            TerminalError::Io(
                "updated executable does not belong to the verified installation".to_owned(),
            )
        })
}

#[cfg(test)]
mod tests {
    use crate::domain::{Installation, InstallationIdentity, InstallationKind};

    use super::{validate_redetected_installation, validate_restart_kind};

    fn installation(kind: InstallationKind, identity_byte: u8) -> Installation {
        Installation {
            identity: InstallationIdentity::from_digest([identity_byte; 32]),
            kind,
            executable: "/tmp/proqi".into(),
            restart_executable: Some("/tmp/proqi".into()),
        }
    }

    #[test]
    fn only_verified_updatable_installation_kinds_can_restart() {
        assert!(validate_restart_kind(InstallationKind::HomebrewFormula).is_ok());
        assert!(validate_restart_kind(InstallationKind::StandaloneArchive).is_ok());
        assert!(validate_restart_kind(InstallationKind::SourceOrUnknown).is_err());
    }

    #[test]
    fn redetection_requires_the_exact_kind_and_identity() {
        let expected = installation(InstallationKind::StandaloneArchive, 1);
        assert!(validate_redetected_installation(&expected, &expected).is_ok());
        assert!(
            validate_redetected_installation(
                &expected,
                &installation(InstallationKind::HomebrewFormula, 1)
            )
            .is_err()
        );
        assert!(
            validate_redetected_installation(
                &expected,
                &installation(InstallationKind::StandaloneArchive, 2)
            )
            .is_err()
        );
    }
}
