//! Deterministic offline source fixture for the immediately preceding schema owner.

#[path = "old_fixture/coordinator.rs"]
mod coordinator;

use std::{
    fs,
    io::Read as _,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::OnceLock,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, kill_process_group};

use proqi::{
    domain::{InstallationIdentity, InstanceId},
    ports::update::InstallDetector as _,
};

pub(super) const OLD_VERSION: &str = "0.8.99";
const PREVIOUS_RELEASE_COMMIT: &str = "9ddc01f1cb2b55dc4f82c587124844a7209aceba";
const COMMAND_OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
const COORDINATOR_TIMEOUT: Duration = Duration::from_secs(90);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const BUILD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

struct CapturedChild {
    child: Option<Child>,
    group: Pid,
    stdout: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
}

impl CapturedChild {
    fn spawn(command: &mut Command, label: &str) -> Self {
        use std::os::unix::process::CommandExt as _;

        let mut child = command
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("{label} spawn failed: {error}"));
        let group = Pid::from_raw(i32::try_from(child.id()).expect("child PID"))
            .expect("positive child PID");
        let stdout = child.stdout.take().expect("captured stdout");
        let stderr = child.stderr.take().expect("captured stderr");
        Self {
            child: Some(child),
            group,
            stdout: Some(thread::spawn(move || read_bounded(stdout))),
            stderr: Some(thread::spawn(move || read_bounded(stderr))),
        }
    }

    fn finish(mut self, timeout: Duration, label: &str) -> Output {
        let deadline = Instant::now() + timeout;
        let status = loop {
            let status = self
                .child
                .as_mut()
                .expect("captured child")
                .try_wait()
                .unwrap_or_else(|error| panic!("{label} wait failed: {error}"));
            if let Some(status) = status {
                break status;
            }
            if Instant::now() >= deadline {
                self.stop();
                panic!("{label} exceeded its {timeout:?} bound");
            }
            thread::sleep(Duration::from_millis(20));
        };
        self.child.take();
        let _terminated_descendants = kill_process_group(self.group, Signal::KILL);
        Output {
            status,
            stdout: join_output(self.stdout.take(), label, "stdout"),
            stderr: join_output(self.stderr.take(), label, "stderr"),
        }
    }

    fn stop(&mut self) {
        let _killed = kill_process_group(self.group, Signal::KILL);
        if let Some(child) = &mut self.child {
            let _killed = child.kill();
            let _reaped = child.wait();
        }
        self.child.take();
    }
}

impl Drop for CapturedChild {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.stop();
        }
        let _stdout = self.stdout.take().map(JoinHandle::join);
        let _stderr = self.stderr.take().map(JoinHandle::join);
    }
}

fn read_bounded(reader: impl std::io::Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .take(COMMAND_OUTPUT_LIMIT.saturating_add(1))
        .read_to_end(&mut output)?;
    Ok(output)
}

fn join_output(
    worker: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    label: &str,
    stream: &str,
) -> Vec<u8> {
    let output = worker
        .expect("capture worker")
        .join()
        .unwrap_or_else(|_| panic!("{label} {stream} capture panicked"))
        .unwrap_or_else(|error| panic!("{label} {stream} capture failed: {error}"));
    assert!(
        output.len() <= usize::try_from(COMMAND_OUTPUT_LIMIT).expect("output limit"),
        "{label} {stream} exceeded its output bound"
    );
    output
}

pub(super) fn run_bounded(command: &mut Command, timeout: Duration, label: &str) -> Output {
    CapturedChild::spawn(command, label).finish(timeout, label)
}

pub(super) struct OldFixture {
    _root: tempfile::TempDir,
    old_binary: PathBuf,
    coordinator: PathBuf,
}

pub(super) struct InstallationFixture {
    pub(super) old_binary: PathBuf,
    pub(super) active_binary: PathBuf,
    pub(super) new_binary: PathBuf,
    pub(super) identity: InstallationIdentity,
}

impl InstallationFixture {
    pub(super) fn replace_externally(&self) {
        fs::remove_file(&self.active_binary).expect("remove old active link");
        symlink(&self.new_binary, &self.active_binary).expect("activate new external binary");
        fs::remove_file(&self.old_binary).expect("remove old Cellar executable");
        assert!(!self.old_binary.exists());
    }
}

impl OldFixture {
    pub(super) fn build() -> &'static Self {
        static FIXTURE: OnceLock<OldFixture> = OnceLock::new();
        FIXTURE.get_or_init(Self::build_uncached)
    }

    fn build_uncached() -> Self {
        let root = tempfile::Builder::new()
            .prefix("proqi-old-schema-source")
            .tempdir_in("/private/tmp")
            .expect("old source root");
        let source = prepare_old_source(root.path());
        let (old_binary, coordinator) = build_old_binaries(&source);
        Self {
            _root: root,
            old_binary,
            coordinator,
        }
    }

    pub(super) fn installation(&self, state: &Path) -> InstallationFixture {
        let prefix = state.join("prefix");
        let old_binary = prefix
            .join("Cellar/proqi")
            .join(OLD_VERSION)
            .join("bin/proqi");
        let new_binary = prefix
            .join("Cellar/proqi")
            .join(env!("CARGO_PKG_VERSION"))
            .join("bin/proqi");
        fs::create_dir_all(old_binary.parent().expect("old binary parent")).expect("old keg");
        fs::create_dir_all(new_binary.parent().expect("new binary parent")).expect("new keg");
        fs::copy(&self.old_binary, &old_binary).expect("install old fixture");
        fs::copy(env!("CARGO_BIN_EXE_proqi"), &new_binary).expect("install new fixture");
        fs::write(
            old_binary
                .parent()
                .and_then(Path::parent)
                .expect("old keg")
                .join("INSTALL_RECEIPT.json"),
            b"{}",
        )
        .expect("old receipt");
        fs::write(
            new_binary
                .parent()
                .and_then(Path::parent)
                .expect("new keg")
                .join("INSTALL_RECEIPT.json"),
            b"{}",
        )
        .expect("new receipt");
        let active_binary = prefix.join("opt/proqi/bin/proqi");
        fs::create_dir_all(active_binary.parent().expect("active parent")).expect("active path");
        symlink(&old_binary, &active_binary).expect("active old binary");
        let detected =
            proqi::adapters::update::SystemInstallDetector::for_executable(old_binary.clone());
        let identity = detected.detect().expect("fixture installation").identity;
        InstallationFixture {
            old_binary,
            active_binary,
            new_binary,
            identity,
        }
    }

    pub(super) fn coordinate(
        &self,
        state: &Path,
        installation: &InstallationFixture,
        initiating: InstanceId,
        initiating_session: proqi::domain::SessionId,
    ) -> serde_json::Value {
        let initiating = initiating.to_string();
        let initiating_session = initiating_session.to_string();
        let installer_active = state.join("installer-active");
        let late_start_observed = state.join("late-start-observed");
        let mut command = Command::new(&self.coordinator);
        command
            .arg(state)
            .arg(&installation.old_binary)
            .arg(&installation.active_binary)
            .arg(&installation.new_binary)
            .arg(&initiating)
            .arg(env!("CARGO_PKG_VERSION"))
            .arg(&installer_active)
            .arg(&late_start_observed);
        let child = CapturedChild::spawn(&mut command, "old coordinator");
        wait_for_path(&installer_active);
        let mut late_command = Command::new(&installation.old_binary);
        late_command
            .args([
                "--state-dir",
                state.to_str().expect("state UTF-8"),
                "--json",
                "-r",
            ])
            .arg(&initiating_session);
        let late = run_bounded(&mut late_command, PROBE_TIMEOUT, "inactive keg start probe");
        let late_output = format!(
            "{}{}",
            String::from_utf8_lossy(&late.stdout),
            String::from_utf8_lossy(&late.stderr)
        );
        assert!(!late.status.success(), "inactive keg entered the schema");
        assert!(late_output.contains("installation_failed"), "{late_output}");
        assert!(
            late_output.contains("does not match the active Homebrew installation"),
            "{late_output}"
        );
        fs::write(&late_start_observed, b"continue").expect("release installer fixture");
        let output = child.finish(COORDINATOR_TIMEOUT, "old coordinator");
        assert!(
            output.status.success(),
            "old coordinator failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("coordinator result")
    }
}

fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "path did not appear: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn prepare_old_source(root: &Path) -> PathBuf {
    let source = root.join("source");
    fs::create_dir(&source).expect("source directory");
    let archive = root.join("previous-release.tar");
    let mut export = Command::new("git");
    export
        .args(["archive", "--format=tar", "--output"])
        .arg(&archive)
        .arg(PREVIOUS_RELEASE_COMMIT)
        .current_dir(repo());
    let output = run_bounded(&mut export, PROBE_TIMEOUT, "previous release source export");
    assert!(
        output.status.success(),
        "previous release source export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut extract = Command::new("/usr/bin/tar");
    extract.args(["-xf"]).arg(&archive).arg("-C").arg(&source);
    let output = run_bounded(
        &mut extract,
        PROBE_TIMEOUT,
        "previous release source extraction",
    );
    assert!(
        output.status.success(),
        "previous release source extraction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // This source is pinned independently of the current test binary. A new
    // release version must not change which version the historical manifest owns.
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(source.join("Cargo.toml")).expect("historical manifest"),
    )
    .expect("valid historical manifest");
    let source_version = manifest["workspace"]["package"]["version"]
        .as_str()
        .expect("historical workspace version");
    let source_version = format!("version = \"{source_version}\"");
    let old_version = format!("version = \"{OLD_VERSION}\"");
    rewrite(
        &source.join("Cargo.toml"),
        &[
            ("members = [\".\", \"xtask\"]", "members = [\".\"]"),
            (&source_version, &old_version),
            (
                "[package]\nname = \"proqi\"",
                "[package]\nname = \"proqi\"\nautobins = false",
            ),
        ],
    );
    fs::write(
        source.join("src/bin/update_fixture.rs"),
        coordinator::SOURCE,
    )
    .expect("write coordinator fixture");
    let manifest = source.join("Cargo.toml");
    let mut content = fs::read_to_string(&manifest).expect("fixture manifest");
    content.push_str(
        "\n[[bin]]\nname = \"proqi_old_fixture\"\npath = \"src/bin/proqi.rs\"\n\n[[bin]]\nname = \"update_fixture\"\npath = \"src/bin/update_fixture.rs\"\n",
    );
    fs::write(manifest, content).expect("write fixture manifest");
    source
}

fn build_old_binaries(source: &Path) -> (PathBuf, PathBuf) {
    let target = shared_fixture_target();
    let mut command = Command::new("cargo");
    command
        .args([
            "build",
            "--offline",
            "--package",
            "proqi",
            "--bin",
            "proqi_old_fixture",
            "--bin",
            "update_fixture",
        ])
        .current_dir(source)
        .env("CARGO_TARGET_DIR", &target);
    let output = run_bounded(&mut command, BUILD_TIMEOUT, "old fixture build");
    assert!(
        output.status.success(),
        "old fixture build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let old_binary = target.join("debug/proqi_old_fixture");
    let coordinator = target.join("debug/update_fixture");
    assert_ne!(
        fs::read(&old_binary).expect("old bytes"),
        fs::read(env!("CARGO_BIN_EXE_proqi")).expect("new bytes")
    );
    (old_binary, coordinator)
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(super) fn shared_fixture_target() -> PathBuf {
    Path::new(env!("CARGO_BIN_EXE_proqi"))
        .parent()
        .and_then(Path::parent)
        .expect("Cargo target directory")
        .to_path_buf()
}

fn rewrite(path: &Path, replacements: &[(&str, &str)]) {
    let mut content = fs::read_to_string(path).expect("rewrite source");
    for (from, to) in replacements {
        assert!(
            content.contains(from),
            "missing fixture rewrite in {}",
            path.display()
        );
        content = content.replacen(from, to, 1);
    }
    fs::write(path, content).expect("write fixture source");
}

#[test]
fn historical_source_preparation_is_independent_of_current_package_version() {
    let root = tempfile::tempdir().expect("isolated historical source");
    let source = prepare_old_source(root.path());
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(source.join("Cargo.toml")).expect("fixture manifest"))
            .expect("valid fixture manifest");
    assert_eq!(
        manifest["workspace"]["package"]["version"].as_str(),
        Some(OLD_VERSION)
    );
    assert_eq!(
        manifest["workspace"]["members"]
            .as_array()
            .expect("members")
            .len(),
        1
    );
    assert_eq!(manifest["package"]["autobins"].as_bool(), Some(false));
    assert!(source.join("src/bin/update_fixture.rs").is_file());
    assert!(source.join("Cargo.lock").is_file());
}
