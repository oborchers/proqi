//! Deterministic offline source fixture for the immediately preceding schema owner.

#[path = "old_fixture/coordinator.rs"]
mod coordinator;

use std::{
    fs,
    io::Read as _,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, kill_process_group};

use proqi::{
    domain::{InstallationIdentity, InstanceId},
    ports::update::InstallDetector as _,
};

const OLD_VERSION: &str = "0.8.99";
const COMMAND_OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
const COORDINATOR_TIMEOUT: Duration = Duration::from_secs(90);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const BUILD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

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

fn run_bounded(command: &mut Command, timeout: Duration, label: &str) -> Output {
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

impl OldFixture {
    pub(super) fn build() -> Self {
        let root = tempfile::Builder::new()
            .prefix("proqi-old-schema-source")
            .tempdir_in("/private/tmp")
            .expect("old source root");
        let source = prepare_old_source(root.path());
        let (old_binary, coordinator) = build_old_binaries(&source, root.path());
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
        let late = run_bounded(&mut late_command, PROBE_TIMEOUT, "obsolete start probe");
        let late_output = format!(
            "{}{}",
            String::from_utf8_lossy(&late.stdout),
            String::from_utf8_lossy(&late.stderr)
        );
        assert!(!late.status.success(), "obsolete start entered the schema");
        assert!(
            late_output.contains("update_convergence_active"),
            "{late_output}"
        );
        assert!(late_output.contains(&initiating_session), "{late_output}");
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
    copy_tree(&repo().join("src"), &source.join("src"));
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "README.md",
        "LICENSE",
        "release-highlights.json",
        "rust-toolchain.toml",
    ] {
        fs::copy(repo().join(file), source.join(file)).expect("copy source file");
    }
    let current_version = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
    let old_version = format!("version = \"{OLD_VERSION}\"");
    rewrite(
        &source.join("Cargo.toml"),
        &[
            ("members = [\".\", \"xtask\"]", "members = [\".\"]"),
            (&current_version, &old_version),
        ],
    );
    rewrite(
        &source.join("src/ports/store.rs"),
        &[
            (
                "SUPPORTED_SCHEMA_VERSION: u32 = 16",
                "SUPPORTED_SCHEMA_VERSION: u32 = 15",
            ),
            (
                "STORAGE_PROTOCOL_VERSION: u32 = 15",
                "STORAGE_PROTOCOL_VERSION: u32 = 14",
            ),
        ],
    );
    rewrite(
        &source.join("src/adapters/sqlite/migration.rs"),
        &[
            (
                "        MIGRATION_14, MIGRATION_15, MIGRATION_16,",
                "        MIGRATION_14, MIGRATION_15,",
            ),
            (
                "        MIGRATION_14,\n        MIGRATION_15,\n        MIGRATION_16,",
                "        MIGRATION_14,\n        MIGRATION_15,",
            ),
        ],
    );
    rewrite(
        &source.join("src/adapters/sqlite/schema.rs"),
        &[(
            "INSERT INTO migration_history(version, applied_at) VALUES (16, 0);\n\";",
            "\";",
        )],
    );
    remove_first_section(
        &source.join("src/adapters/sqlite/schema.rs"),
        "CREATE TABLE browser_history_state (",
        "INSERT INTO browser_history_state(singleton, cursor) VALUES (1, 0);\n\n",
    );
    remove_browser_history_dependencies(&source);
    fs::write(
        source.join("src/bin/update_fixture.rs"),
        coordinator::SOURCE,
    )
    .expect("write coordinator fixture");
    source
}

fn build_old_binaries(source: &Path, fixture_root: &Path) -> (PathBuf, PathBuf) {
    let target = fixture_root.join("target");
    let mut command = Command::new("cargo");
    command
        .args([
            "build",
            "--offline",
            "--package",
            "proqi",
            "--bin",
            "proqi",
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
    let old_binary = target.join("debug/proqi");
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

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("copy target");
    for entry in fs::read_dir(source).expect("source directory") {
        let entry = entry.expect("source entry");
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &destination);
        } else {
            fs::copy(path, destination).expect("copy source entry");
        }
    }
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

fn remove_first_section(path: &Path, start: &str, end: &str) {
    let mut content = fs::read_to_string(path).expect("read section source");
    let start_index = content.find(start).expect("fixture section start");
    let end_index = content[start_index..]
        .find(end)
        .map(|offset| start_index + offset + end.len())
        .expect("fixture section end");
    content.replace_range(start_index..end_index, "");
    fs::write(path, content).expect("write section source");
}

fn remove_browser_history_dependencies(source: &Path) {
    rewrite(
        &source.join("src/adapters/sqlite/board_commit.rs"),
        &[
            (
                "    super::browser_history::ensure_not_used_by_browser_history(\n        transaction,\n        operation.id.database_bytes(),\n    )?;\n",
                "",
            ),
            (
                "    super::browser_history::invalidate_activity_conflicts(transaction, operation.session_id)?;\n",
                "",
            ),
            (
                "    super::browser_history::ensure_not_used_by_browser_history(\n        transaction,\n        revision.id.database_bytes(),\n    )?;\n",
                "",
            ),
        ],
    );
    rewrite(
        &source.join("src/adapters/sqlite/history_commit.rs"),
        &[
            (
                "    super::browser_history::ensure_not_used_by_browser_history(\n        transaction,\n        operation_id.database_bytes(),\n    )?;\n",
                "",
            ),
            (
                "    super::browser_history::invalidate_activity_conflicts(transaction, session_id)?;\n",
                "",
            ),
        ],
    );
    rewrite(
        &source.join("src/adapters/sqlite/session_admin.rs"),
        &[(
            "    super::browser_history::invalidate_activity_conflicts(transaction, id)?;\n",
            "",
        )],
    );
}
