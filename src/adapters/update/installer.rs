//! Exact, shell-free Homebrew formula installation adapter.

use std::{ffi::OsString, path::PathBuf, time::Duration};

use crate::ports::{
    environment::{ProcessRequest, ProcessRunner},
    update::{HomebrewInstaller, UpdateError},
};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Runs the one supported Homebrew formula upgrade through an injected process boundary.
pub struct HomebrewFormulaInstaller<'a, R> {
    runner: &'a mut R,
    active_executable: PathBuf,
}

impl<'a, R> HomebrewFormulaInstaller<'a, R> {
    /// Bind the exact command adapter to a direct process runner.
    #[must_use]
    pub fn new(runner: &'a mut R, active_executable: PathBuf) -> Self {
        Self {
            runner,
            active_executable,
        }
    }
}

impl<R: ProcessRunner> HomebrewInstaller for HomebrewFormulaInstaller<'_, R> {
    fn upgrade(
        &mut self,
        expected: &crate::domain::StableVersion,
    ) -> Result<crate::domain::StableVersion, UpdateError> {
        let output = self
            .runner
            .run(ProcessRequest {
                program: OsString::from("brew"),
                args: ["upgrade", "--formula", "oborchers/tap/proqi"]
                    .map(OsString::from)
                    .to_vec(),
                stdin: None,
                timeout: INSTALL_TIMEOUT,
            })
            .map_err(|_| UpdateError::InstallerFailed)?;
        if output.exit_code != Some(0) {
            return Err(UpdateError::InstallerFailed);
        }
        verify_installed_version(self.runner, &self.active_executable, expected)
    }
}

pub(crate) fn verify_installed_version<R: ProcessRunner>(
    runner: &mut R,
    executable: &std::path::Path,
    expected: &crate::domain::StableVersion,
) -> Result<crate::domain::StableVersion, UpdateError> {
    let output = runner
        .run(ProcessRequest {
            program: executable.as_os_str().to_owned(),
            args: vec![OsString::from("--version")],
            stdin: None,
            timeout: Duration::from_secs(10),
        })
        .map_err(|_| UpdateError::InstallerFailed)?;
    if output.exit_code != Some(0) {
        return Err(UpdateError::InstallerFailed);
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| UpdateError::InstallerFailed)?;
    let version = text
        .trim()
        .strip_prefix("proqi ")
        .ok_or(UpdateError::InstallerFailed)
        .and_then(|value| {
            crate::domain::StableVersion::parse(value).map_err(|_| UpdateError::InstallerFailed)
        })?;
    (&version == expected)
        .then_some(version)
        .ok_or(UpdateError::InstallerFailed)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::{
        adapters::memory::FakeProcessRunner,
        ports::{
            environment::ProcessOutput,
            update::{HomebrewInstaller as _, UpdateError},
        },
    };

    use super::HomebrewFormulaInstaller;

    #[test]
    fn executes_only_the_exact_formula_upgrade_without_a_shell() {
        let mut runner = FakeProcessRunner {
            requests: Vec::new(),
            results: VecDeque::from([
                Ok(ProcessOutput {
                    exit_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                }),
                Ok(ProcessOutput {
                    exit_code: Some(0),
                    stdout: b"proqi 0.2.0\n".to_vec(),
                    stderr: Vec::new(),
                }),
            ]),
        };
        let expected = crate::domain::StableVersion::parse("0.2.0").expect("version");
        HomebrewFormulaInstaller::new(&mut runner, "/opt/homebrew/opt/proqi/bin/proqi".into())
            .upgrade(&expected)
            .expect("upgrade");
        let request = &runner.requests[0];
        assert_eq!(runner.requests.len(), 2);
        assert_eq!(request.program, "brew");
        assert_eq!(
            request.args,
            ["upgrade", "--formula", "oborchers/tap/proqi"].map(std::ffi::OsString::from)
        );
        assert!(request.stdin.is_none());
    }

    #[test]
    fn nonzero_or_ambiguous_exit_never_reports_success() {
        for exit_code in [Some(1), None] {
            let mut runner = FakeProcessRunner {
                requests: Vec::new(),
                results: VecDeque::from([Ok(ProcessOutput {
                    exit_code,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })]),
            };
            assert_eq!(
                HomebrewFormulaInstaller::new(
                    &mut runner,
                    "/opt/homebrew/opt/proqi/bin/proqi".into(),
                )
                .upgrade(&crate::domain::StableVersion::parse("0.2.0").expect("version")),
                Err(UpdateError::InstallerFailed)
            );
        }
    }
}
