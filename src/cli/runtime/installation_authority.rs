//! Active-installation and executable-byte authority for startup convergence.

use std::path::Path;

use crate::{
    adapters::{
        runtime::{FileRuntimeCoordinator, input_recovery::ExecutableIdentity},
        update::SystemInstallDetector,
    },
    domain::{Installation, SessionId, StableVersion, Timestamp},
    ports::{
        environment::IdGenerator,
        runtime::{Lease, RuntimeCoordinator, UpdateReplacementContext},
        update::{
            ExternalUpgradeAdoptionAuthority, ExternalUpgradeAuthorityError, InstallDetector as _,
            UpdateError,
        },
    },
};

use super::{
    CliError,
    update_admission::{StartupAdmission, StartupExecutable, admit_update_start},
};

#[expect(
    clippy::too_many_arguments,
    reason = "startup composition keeps executable, installation, identity, and time explicit"
)]
pub(super) fn admit_runtime_startup(
    cache_dir: &Path,
    coordinator: &FileRuntimeCoordinator,
    executable: &Path,
    installation: &Installation,
    current: &StableVersion,
    initial_executable_identity: Option<ExecutableIdentity>,
    replacement: Option<&UpdateReplacementContext>,
    exact_resume: Option<SessionId>,
    ids: &mut impl IdGenerator,
    now: Timestamp,
) -> Result<(StartupAdmission, Option<ExecutableIdentity>), CliError> {
    let mut executable_identity = initial_executable_identity;
    let admission = {
        let mut authority = RuntimeExternalUpgradeAuthority {
            coordinator,
            executable,
            installation,
            executable_identity: &mut executable_identity,
        };
        admit_update_start(
            cache_dir,
            coordinator,
            installation,
            exact_resume,
            StartupExecutable::new(current, replacement, &mut authority),
            ids,
            now,
        )?
    };
    Ok((admission, executable_identity))
}

#[derive(Clone, Copy)]
pub(super) struct VerifiedStartupInstallation<'a> {
    executable: &'a Path,
    installation: &'a Installation,
    executable_identity: Option<&'a ExecutableIdentity>,
}

impl<'a> VerifiedStartupInstallation<'a> {
    pub(super) const fn new(
        executable: &'a Path,
        installation: &'a Installation,
        executable_identity: Option<&'a ExecutableIdentity>,
    ) -> Self {
        Self {
            executable,
            installation,
            executable_identity,
        }
    }

    pub(super) fn revalidate(self) -> Result<(), CliError> {
        self.revalidate_for_update()
            .map_err(|error| CliError::new("installation_failed", error.to_string(), 1))
    }

    fn revalidate_for_update(self) -> Result<(), UpdateError> {
        let detected =
            SystemInstallDetector::for_executable(self.executable.to_path_buf()).detect()?;
        let executable_unchanged = self.executable_identity.is_none_or(|expected| {
            ExecutableIdentity::read(self.executable).is_ok_and(|identity| identity == *expected)
        });
        (detected == *self.installation && executable_unchanged)
            .then_some(())
            .ok_or_else(|| {
                UpdateError::Installation(
                    "the verified active Proqi installation changed during startup; retry with the active executable"
                        .to_owned(),
                )
            })
    }
}

pub(super) struct RuntimeExternalUpgradeAuthority<'a> {
    pub(super) coordinator: &'a FileRuntimeCoordinator,
    pub(super) executable: &'a Path,
    pub(super) installation: &'a Installation,
    pub(super) executable_identity: &'a mut Option<ExecutableIdentity>,
}

impl ExternalUpgradeAdoptionAuthority for RuntimeExternalUpgradeAuthority<'_> {
    fn establish(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        VerifiedStartupInstallation::new(
            self.executable,
            self.installation,
            self.executable_identity.as_ref(),
        )
        .revalidate_for_update()?;
        let identity = ExecutableIdentity::read(self.executable).map_err(|_| {
            UpdateError::Installation(
                "the verified active Proqi executable became unavailable during convergence"
                    .to_owned(),
            )
        })?;
        if self
            .executable_identity
            .as_ref()
            .is_some_and(|expected| *expected != identity)
        {
            return Err(UpdateError::Installation(
                "the verified active Proqi executable changed during convergence".to_owned(),
            )
            .into());
        }
        *self.executable_identity = Some(identity);
        Ok(())
    }

    fn revalidate_installation(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        let expected = self.executable_identity.as_ref().ok_or_else(|| {
            UpdateError::Installation(
                "external convergence executable identity was not established".to_owned(),
            )
        })?;
        VerifiedStartupInstallation::new(self.executable, self.installation, Some(expected))
            .revalidate_for_update()
            .map_err(Into::into)
    }

    fn acquire(&mut self) -> Result<Box<dyn Lease>, ExternalUpgradeAuthorityError> {
        let schema = self.coordinator.acquire_schema_exclusive()?;
        self.revalidate_installation()?;
        Ok(Box::new(schema))
    }
}
