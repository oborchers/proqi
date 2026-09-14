//! Verified release-attached installer download and standalone replacement.

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
    time::Duration,
};

use sha2::{Digest as _, Sha256};
use ureq::Agent;

use crate::{
    domain::StableVersion,
    ports::{
        environment::{ProcessRequest, ProcessRunner},
        update::{StandaloneInstallerSource, UpdateError, UpdateInstaller},
    },
};

use super::installer::verify_installed_version;

const RELEASE_ROOT: &str = "https://github.com/oborchers/proqi/releases/download";
const INSTALLER_NAME: &str = "proqi-installer.sh";
const CHECKSUM_NAME: &str = "proqi-installer.sh.sha256";
const MAX_INSTALLER_BYTES: u64 = 128 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 512;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Downloads an exact installer and releases it only after checksum verification.
#[derive(Clone)]
pub struct GitHubStandaloneInstallerSource {
    agent: Agent,
}

impl GitHubStandaloneInstallerSource {
    /// Construct a bounded HTTPS-only release client.
    #[must_use]
    pub fn new() -> Self {
        let config = Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(3)
            .max_response_header_size(16 * 1024)
            .timeout_global(Some(DOWNLOAD_TIMEOUT))
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(DOWNLOAD_TIMEOUT))
            .timeout_recv_body(Some(DOWNLOAD_TIMEOUT))
            .user_agent(format!("proqi/{}", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
        }
    }

    fn download(&self, url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|_| UpdateError::Network)?;
        if response.status().as_u16() != 200 {
            return Err(UpdateError::InvalidResponse);
        }
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|error| {
                if matches!(error, ureq::Error::BodyExceedsLimit(_)) {
                    UpdateError::ResponseTooLarge
                } else {
                    UpdateError::Network
                }
            })
    }
}

impl Default for GitHubStandaloneInstallerSource {
    fn default() -> Self {
        Self::new()
    }
}

impl StandaloneInstallerSource for GitHubStandaloneInstallerSource {
    fn verified_installer(&mut self, expected: &StableVersion) -> Result<Vec<u8>, UpdateError> {
        let root = format!("{RELEASE_ROOT}/{}", expected.tag());
        let checksum = self.download(&format!("{root}/{CHECKSUM_NAME}"), MAX_CHECKSUM_BYTES)?;
        let installer = self.download(&format!("{root}/{INSTALLER_NAME}"), MAX_INSTALLER_BYTES)?;
        verify_checksum(&checksum, &installer)?;
        Ok(installer)
    }
}

/// Invokes one checksum-verified release installer for the existing standalone prefix.
pub struct StandaloneArchiveInstaller<'a, R, S> {
    runner: &'a mut R,
    source: &'a mut S,
    installation_root: PathBuf,
    active_executable: PathBuf,
}

impl<'a, R, S> StandaloneArchiveInstaller<'a, R, S> {
    /// Bind replacement to one verified standalone installation root.
    #[must_use]
    pub fn new(
        runner: &'a mut R,
        source: &'a mut S,
        installation_root: PathBuf,
        active_executable: PathBuf,
    ) -> Self {
        Self {
            runner,
            source,
            installation_root,
            active_executable,
        }
    }
}

impl<R: ProcessRunner, S: StandaloneInstallerSource> UpdateInstaller
    for StandaloneArchiveInstaller<'_, R, S>
{
    fn upgrade(&mut self, expected: &StableVersion) -> Result<StableVersion, UpdateError> {
        let bytes = self.source.verified_installer(expected)?;
        let script = self
            .installation_root
            .join(format!(".proqi-installer.{}", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&script)
            .map_err(|_| UpdateError::InstallerFailed)?;
        let cleanup = TemporaryInstaller(script.clone());
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| UpdateError::InstallerFailed)?;
        drop(file);
        let output = self
            .runner
            .run(ProcessRequest {
                program: OsString::from("sh"),
                args: vec![
                    script.into_os_string(),
                    OsString::from("--version"),
                    OsString::from(expected.tag()),
                    OsString::from("--prefix"),
                    self.installation_root.as_os_str().to_owned(),
                ],
                stdin: None,
                timeout: INSTALL_TIMEOUT,
            })
            .map_err(|_| UpdateError::InstallerFailed)?;
        if output.exit_code != Some(0) {
            return Err(UpdateError::InstallerFailed);
        }
        let version = verify_installed_version(self.runner, &self.active_executable, expected);
        drop(cleanup);
        version
    }
}

fn verify_checksum(checksum: &[u8], installer: &[u8]) -> Result<(), UpdateError> {
    let text = std::str::from_utf8(checksum).map_err(|_| UpdateError::InvalidResponse)?;
    let record = text
        .strip_suffix('\n')
        .ok_or(UpdateError::InvalidResponse)?;
    if record.contains(['\n', '\r']) {
        return Err(UpdateError::InvalidResponse);
    }
    let mut fields = record.split_whitespace();
    let expected = fields.next().ok_or(UpdateError::InvalidResponse)?;
    let name = fields.next().ok_or(UpdateError::InvalidResponse)?;
    if fields.next().is_some()
        || name != INSTALLER_NAME
        || expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(UpdateError::InvalidResponse);
    }
    let actual = hex_digest(Sha256::digest(installer).as_slice());
    (actual == expected.to_ascii_lowercase())
        .then_some(())
        .ok_or(UpdateError::InstallerFailed)
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

struct TemporaryInstaller(PathBuf);

impl Drop for TemporaryInstaller {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, ffi::OsString};

    use sha2::Digest as _;

    use crate::{
        adapters::memory::FakeProcessRunner,
        ports::{
            environment::ProcessOutput,
            update::{StandaloneInstallerSource, UpdateError, UpdateInstaller as _},
        },
    };

    use super::{StandaloneArchiveInstaller, verify_checksum};

    struct Source(Result<Vec<u8>, UpdateError>);

    impl StandaloneInstallerSource for Source {
        fn verified_installer(
            &mut self,
            _: &crate::domain::StableVersion,
        ) -> Result<Vec<u8>, UpdateError> {
            self.0.clone()
        }
    }

    #[test]
    fn checksum_record_is_exact_and_detects_tampering() {
        let digest = super::hex_digest(sha2::Sha256::digest(b"script").as_slice());
        assert_eq!(
            verify_checksum(
                format!("{digest}  proqi-installer.sh\n").as_bytes(),
                b"script"
            ),
            Ok(())
        );
        for invalid in [
            format!("{digest}  wrong.sh\n"),
            format!("{digest}  proqi-installer.sh\nextra\n"),
            "bad  proqi-installer.sh\n".to_owned(),
        ] {
            assert_eq!(
                verify_checksum(invalid.as_bytes(), b"script"),
                Err(UpdateError::InvalidResponse)
            );
        }
        assert_eq!(
            verify_checksum(
                format!("{digest}  proqi-installer.sh\n").as_bytes(),
                b"tampered"
            ),
            Err(UpdateError::InstallerFailed)
        );
    }

    #[test]
    fn exact_verified_installer_runs_before_version_confirmation() {
        let temporary = tempfile::tempdir().expect("installation");
        let executable = temporary.path().join("proqi");
        let mut source = Source(Ok(b"verified installer".to_vec()));
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
                    stdout: b"proqi 1.2.3\n".to_vec(),
                    stderr: Vec::new(),
                }),
            ]),
        };
        let expected = crate::domain::StableVersion::parse("1.2.3").expect("version");
        StandaloneArchiveInstaller::new(
            &mut runner,
            &mut source,
            temporary.path().to_path_buf(),
            executable,
        )
        .upgrade(&expected)
        .expect("upgrade");
        assert_eq!(runner.requests.len(), 2);
        assert_eq!(runner.requests[0].program, "sh");
        assert!(runner.requests[0].args.contains(&OsString::from("v1.2.3")));
        assert_eq!(runner.requests[1].args, [OsString::from("--version")]);
    }

    #[test]
    fn unverified_installer_and_failed_replacement_never_report_success() {
        let temporary = tempfile::tempdir().expect("installation");
        let executable = temporary.path().join("proqi");
        std::fs::write(&executable, b"old binary").expect("old executable");
        let expected = crate::domain::StableVersion::parse("1.2.3").expect("version");
        let mut source = Source(Err(UpdateError::InstallerFailed));
        let mut runner = FakeProcessRunner::default();
        let result = StandaloneArchiveInstaller::new(
            &mut runner,
            &mut source,
            temporary.path().to_path_buf(),
            executable.clone(),
        )
        .upgrade(&expected);
        assert_eq!(result, Err(UpdateError::InstallerFailed));
        assert!(runner.requests.is_empty());
        assert_eq!(
            std::fs::read(&executable).expect("old executable"),
            b"old binary"
        );

        let mut source = Source(Ok(b"verified installer".to_vec()));
        let mut runner = FakeProcessRunner {
            requests: Vec::new(),
            results: VecDeque::from([Ok(ProcessOutput {
                exit_code: Some(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })]),
        };
        let result = StandaloneArchiveInstaller::new(
            &mut runner,
            &mut source,
            temporary.path().to_path_buf(),
            executable,
        )
        .upgrade(&expected);
        assert_eq!(result, Err(UpdateError::InstallerFailed));
        assert_eq!(runner.requests.len(), 1);
    }
}
