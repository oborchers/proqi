use std::{fs, os::unix::fs::PermissionsExt as _, process::Command};

use proqi::{
    adapters::update::StandaloneArchiveInstaller,
    domain::StableVersion,
    ports::{
        environment::{ProcessError, ProcessOutput, ProcessRequest, ProcessRunner},
        update::{StandaloneInstallerSource, UpdateError, UpdateInstaller as _},
    },
};

use super::{Fixture, VERSION, assert_existing, assert_updatable_installation, render};

struct GeneratedInstallerSource {
    bytes: Vec<u8>,
    requested: Vec<StableVersion>,
}

impl StandaloneInstallerSource for GeneratedInstallerSource {
    fn verified_installer(&mut self, expected: &StableVersion) -> Result<Vec<u8>, UpdateError> {
        self.requested.push(expected.clone());
        Ok(self.bytes.clone())
    }
}

struct IsolatedProcessRunner<'a> {
    fixture: &'a Fixture,
    requests: Vec<ProcessRequest>,
}

impl ProcessRunner for IsolatedProcessRunner<'_> {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        if request.stdin.is_some() {
            return Err(ProcessError::Io(
                "installer update contract does not accept standard input".to_owned(),
            ));
        }
        let output = Command::new(&request.program)
            .args(&request.args)
            .env("PATH", &self.fixture.path)
            .env("HOME", &self.fixture.home)
            .env("PROQI_INSTALL_DIR", &self.fixture.destination)
            .env("PROQI_TEST_ASSETS", &self.fixture.assets)
            .env("PROQI_TEST_OS", "Linux")
            .env("PROQI_TEST_CPU", "x86_64")
            .env("PROQI_TEST_LIBC", "gnu")
            .env("PROQI_TEST_LATEST", VERSION)
            .env("PROQI_TEST_DESTINATION", &self.fixture.destination)
            .output()
            .map_err(|error| ProcessError::Io(error.to_string()))?;
        self.requests.push(request);
        Ok(ProcessOutput {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

#[test]
fn updater_executes_the_generated_installer_and_verifies_the_replacement() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.destination).expect("installation destination");
    let executable = fixture.destination.join("proqi");
    fs::write(&executable, "#!/bin/sh\nprintf 'proqi 1.2.2\\n'\n").expect("old binary");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
        .expect("old binary permissions");
    fs::write(
        fixture.destination.join("proqi-installation.json"),
        crate::package::INSTALL_MARKER,
    )
    .expect("standalone marker");

    let expected = StableVersion::parse("1.2.3").expect("expected version");
    let mut source = GeneratedInstallerSource {
        bytes: render(VERSION).expect("generated installer").into_bytes(),
        requested: Vec::new(),
    };
    let mut runner = IsolatedProcessRunner {
        fixture: &fixture,
        requests: Vec::new(),
    };
    let installed = StandaloneArchiveInstaller::new(
        &mut runner,
        &mut source,
        fixture.destination.clone(),
        executable,
    )
    .upgrade(&expected)
    .expect("standalone update");

    assert_eq!(installed, expected);
    assert_eq!(source.requested, [expected]);
    assert_eq!(runner.requests.len(), 2);
    assert_eq!(runner.requests[0].program, "sh");
    assert_eq!(runner.requests[1].args, ["--version"]);
    assert_updatable_installation(&fixture);
    assert!(
        fs::read_dir(&fixture.destination)
            .expect("installation members")
            .all(|entry| !entry
                .expect("installation member")
                .file_name()
                .to_string_lossy()
                .starts_with(".proqi-installer."))
    );
}

#[test]
fn wrong_architecture_and_wrong_libc_binaries_never_replace_the_existing_installation() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.destination).expect("installation destination");
    fs::write(fixture.destination.join("proqi"), "existing\n").expect("existing binary");

    for incompatibility in ["wrong-architecture", "wrong-libc"] {
        fixture.assert_failure(
            &[],
            &[("PROQI_TEST_TAR", incompatibility)],
            "verified executable did not start on this platform",
        );
        assert_existing(&fixture);
        assert!(!fixture.destination.join("proqi-installation.json").exists());
    }
}
