//! Safe Unix PTY coverage for the copied release executable.

use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::symlink,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};
use proqi::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        update::{FileUpdateStateStore, SystemInstallDetector},
    },
    application::{UpdateCheckMode, UpdateRefresh, UpdateRestartCoordinator, UpdateService},
    domain::{InstallationIdentity, InstallationKind, InstanceId, StableVersion, Timestamp},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::InstanceInfo,
        update::{
            HomebrewInstaller, InstallDetector as _, ReleaseObservation, ReleaseSource,
            UPDATE_CONTROL_PROTOCOL_VERSION, UpdateError, UpdateInstanceRegistry as _,
            UpdateParticipantGateway as _, UpdatePrepareReply, UpdatePrepareRequest,
            UpdateRestartRequest,
        },
    },
};
use serde_json::Value;

use super::{InstalledProduct, parse_success};

struct PtyChild {
    child: Box<dyn Child + Send + Sync>,
    input: Box<dyn Write + Send>,
    output: Arc<Mutex<Vec<u8>>>,
    reader: thread::JoinHandle<()>,
}

pub(super) fn assert_active_owner_and_terminal_restoration(
    product: &InstalledProduct,
    session: &str,
) {
    let mut owner = spawn(product, session);
    wait_for_owner(product, session, &mut owner);
    let forwarded = json_input(
        product,
        &["thoughts", "add", session],
        "forwarded through the installed owner Grüße 界",
    );
    assert!(
        forwarded["data"]["thought_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("tht_"))
    );
    owner.input.write_all(b"q").expect("quit owner");
    owner.input.flush().expect("flush quit");
    let output = finish(owner, Duration::from_secs(10));
    assert_terminal_restored(&output);

    let created = product.json(&[]);
    let terminated_session = created["data"]["session_id"]
        .as_str()
        .expect("termination session");
    let mut owner = spawn(product, terminated_session);
    wait_for_owner(product, terminated_session, &mut owner);
    let process_id = owner.child.process_id().expect("PTY process ID");
    let termination = Command::new("/bin/kill")
        .args(["-TERM", &process_id.to_string()])
        .status()
        .expect("signal PTY owner");
    assert!(termination.success());
    let output = finish(owner, Duration::from_secs(10));
    assert_terminal_restored(&output);
    let resumed = product.json(&["-r", terminated_session]);
    assert_eq!(resumed["data"]["session_id"], terminated_session);
    assert_update_contract(product);
}

struct FakeSource {
    calls: usize,
}

impl ReleaseSource for FakeSource {
    fn latest_stable(
        &mut self,
        _installation: InstallationKind,
        _etag: Option<&str>,
    ) -> Result<ReleaseObservation, UpdateError> {
        self.calls = self.calls.saturating_add(1);
        Ok(ReleaseObservation::Latest {
            version: StableVersion::parse("0.1.0").expect("package version"),
            etag: Some("package-smoke".to_owned()),
        })
    }
}

struct FakeInstaller {
    fail: bool,
    calls: usize,
}

impl HomebrewInstaller for FakeInstaller {
    fn upgrade(&mut self, expected: &StableVersion) -> Result<StableVersion, UpdateError> {
        self.calls = self.calls.saturating_add(1);
        if self.fail {
            Err(UpdateError::InstallerFailed)
        } else {
            Ok(expected.clone())
        }
    }
}

fn assert_update_contract(product: &InstalledProduct) {
    let homebrew = fake_homebrew_product(product, "update-success");
    let created = homebrew.json(&[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("update session");
    let mut owner = spawn(&homebrew, session);
    wait_for_owner(&homebrew, session, &mut owner);
    let installation = SystemInstallDetector::for_executable(homebrew.binary.clone())
        .detect()
        .expect("fake Homebrew installation");
    assert_fake_update_services(&homebrew, session, installation.identity);
    let registry = update_registry(&homebrew, installation.identity);
    let version = StableVersion::parse("0.1.0").expect("package version");
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let mut ids = SystemIdGenerator;
    let before = active_participant(&registry, session).expect("active package participant");
    let operation = ids.request_id();
    let ready = gateway
        .prepare(
            &before,
            &UpdatePrepareRequest {
                operation_id: operation,
                target_version: version.clone(),
                installation_identity: installation.identity,
                deadline,
            },
        )
        .expect("prepare installed owner");
    assert!(matches!(ready, UpdatePrepareReply::Ready { .. }));
    assert!(
        gateway
            .restart(
                &before,
                &UpdateRestartRequest {
                    operation_id: operation,
                    installed_version: version,
                },
            )
            .expect("request same-PTY replacement")
            .accepted
    );
    let after = wait_for_replacement(&registry, session, before.instance_id);
    assert_eq!(after.pid, before.pid);
    owner.input.write_all(b"q").expect("quit replaced owner");
    owner.input.flush().expect("flush replaced quit");
    assert_terminal_restored(&finish(owner, Duration::from_secs(10)));
    assert_failed_exec_is_recoverable(product);
}

fn assert_fake_update_services(
    homebrew: &InstalledProduct,
    session: &str,
    installation: InstallationIdentity,
) {
    let state = FileUpdateStateStore::new(&homebrew.state.join("cache")).expect("update state");
    let version = StableVersion::parse("0.1.0").expect("package version");
    let mut source = FakeSource { calls: 0 };
    let checked = UpdateService::new(
        &state,
        &mut source,
        &SystemInstallDetector::for_executable(homebrew.binary.clone()),
        &SystemClock,
    )
    .check(version.clone(), UpdateCheckMode::Explicit)
    .expect("fake release check");
    assert_eq!(checked.refresh, UpdateRefresh::Refreshed);
    assert_eq!(source.calls, 1);

    let registry = update_registry(homebrew, installation);
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let mut ids = SystemIdGenerator;
    let mut failing = FakeInstaller {
        fail: true,
        calls: 0,
    };
    let failure = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut failing)
        .execute(ids.request_id(), installation, &version, deadline);
    assert!(matches!(failure, Err(UpdateError::InstallerFailed)));
    assert_eq!(failing.calls, 1);
    let usable = json_input(
        homebrew,
        &["thoughts", "add", session],
        "installer failure kept this session usable",
    );
    assert_eq!(usable["data"]["receipt"]["idempotent_replay"], false);
    let mut succeeding = FakeInstaller {
        fail: false,
        calls: 0,
    };
    let installed = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut succeeding)
        .execute(ids.request_id(), installation, &version, deadline)
        .expect("coordinate one fake installation");
    assert_eq!(succeeding.calls, 1);
    assert_eq!(installed.prepared_participants, 1);
    assert_eq!(installed.restart_requests, 0);
}

fn assert_failed_exec_is_recoverable(product: &InstalledProduct) {
    let homebrew = fake_homebrew_product(product, "update-exec-failure");
    let created = homebrew.json(&[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("failed-exec session");
    let mut owner = spawn(&homebrew, session);
    wait_for_owner(&homebrew, session, &mut owner);
    let installation = SystemInstallDetector::for_executable(homebrew.binary.clone())
        .detect()
        .expect("failed-exec installation");
    let registry = update_registry(&homebrew, installation.identity);
    let before = active_participant(&registry, session).expect("active package participant");
    let mut ids = SystemIdGenerator;
    let operation = ids.request_id();
    let version = StableVersion::parse("0.1.0").expect("package version");
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let ready = gateway
        .prepare(
            &before,
            &UpdatePrepareRequest {
                operation_id: operation,
                target_version: version.clone(),
                installation_identity: installation.identity,
                deadline,
            },
        )
        .expect("prepare failed-exec owner");
    assert!(matches!(ready, UpdatePrepareReply::Ready { .. }));
    fs::remove_file(
        homebrew
            .binary
            .parent()
            .expect("keg bin")
            .parent()
            .expect("keg")
            .parent()
            .expect("formula")
            .parent()
            .expect("Cellar")
            .parent()
            .expect("prefix")
            .join("opt/proqi/bin/proqi"),
    )
    .expect("remove test-owned active link");
    assert!(
        gateway
            .restart(
                &before,
                &UpdateRestartRequest {
                    operation_id: operation,
                    installed_version: version,
                },
            )
            .expect("accept failed replacement")
            .accepted
    );
    let (success, output) = finish_with_status(owner, Duration::from_secs(10));
    assert!(
        !success,
        "injected Unix exec failure unexpectedly succeeded"
    );
    assert_terminal_restored(&output);
    let resumed = homebrew.json(&["-r", session]);
    assert_eq!(resumed["data"]["session_id"], session);
}

fn fake_homebrew_product(product: &InstalledProduct, name: &str) -> InstalledProduct {
    let root = product.state.join(name);
    let prefix = root.join("prefix");
    let keg = prefix.join("Cellar/proqi/0.1.0");
    let binary = keg.join("bin/proqi");
    fs::create_dir_all(binary.parent().expect("fake keg bin")).expect("create fake Homebrew keg");
    fs::copy(&product.binary, &binary).expect("copy package binary into fake keg");
    fs::write(keg.join("INSTALL_RECEIPT.json"), b"{}").expect("write fake receipt");
    let active = prefix.join("opt/proqi/bin/proqi");
    fs::create_dir_all(active.parent().expect("active bin")).expect("create active bin");
    symlink(&binary, active).expect("link active package binary");
    let state = root.join("state");
    for directory in ["config", "data", "cache", "runtime"] {
        fs::create_dir_all(state.join(directory)).expect("create update state directory");
    }
    fs::write(
        state.join("config/config.toml"),
        b"check_for_updates = false\n",
    )
    .expect("disable implicit update source");
    InstalledProduct {
        binary,
        archive: product.archive.clone(),
        state,
        working: product.working.clone(),
    }
}

fn update_registry(
    product: &InstalledProduct,
    installation: InstallationIdentity,
) -> FileRuntimeCoordinator {
    let mut ids = SystemIdGenerator;
    FileRuntimeCoordinator::new(
        product.state.join("runtime"),
        ids.instance_id(),
        product.working.clone(),
        SystemClock.now(),
        "0.1.0",
    )
    .expect("package update registry")
    .with_update_context(installation, UPDATE_CONTROL_PROTOCOL_VERSION)
}

fn active_participant(registry: &FileRuntimeCoordinator, session: &str) -> Option<InstanceInfo> {
    registry
        .active_instances()
        .expect("scan package participants")
        .into_iter()
        .find(|participant| participant.session_id.to_string() == session)
}

fn wait_for_replacement(
    registry: &FileRuntimeCoordinator,
    session: &str,
    previous: InstanceId,
) -> InstanceInfo {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(participant) = active_participant(registry, session)
            && participant.instance_id != previous
        {
            return participant;
        }
        assert!(Instant::now() < deadline, "installed owner did not exec");
        thread::sleep(Duration::from_millis(20));
    }
}

fn spawn(product: &InstalledProduct, session: &str) -> PtyChild {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open package PTY");
    let reader = pair.master.try_clone_reader().expect("clone PTY reader");
    let input = pair.master.take_writer().expect("take PTY writer");
    let output = Arc::new(Mutex::new(Vec::new()));
    let reader_output = Arc::clone(&output);
    let reader = thread::spawn(move || read_pty(reader, &reader_output));
    let mut command = CommandBuilder::new(&product.binary);
    command.arg("--state-dir");
    command.arg(&product.state);
    command.args(["-r", session]);
    command.cwd(&product.working);
    command.env("PROQI_DISABLE_HERDR", "1");
    command.env("NO_PROXY", "*");
    command.env("HTTP_PROXY", "http://127.0.0.1:1");
    command.env("HTTPS_PROXY", "http://127.0.0.1:1");
    command.env("TERM", "xterm-256color");
    let child = pair
        .slave
        .spawn_command(command)
        .expect("spawn installed PTY owner");
    drop(pair.slave);
    PtyChild {
        child,
        input,
        output,
        reader,
    }
}

fn read_pty(mut reader: Box<dyn Read + Send>, output: &Mutex<Vec<u8>>) {
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => output
                .lock()
                .expect("PTY output lock")
                .extend_from_slice(&buffer[..read]),
        }
    }
}

fn finish(owner: PtyChild, timeout: Duration) -> Vec<u8> {
    let (success, output) = finish_with_status(owner, timeout);
    assert!(success, "PTY owner exited unsuccessfully");
    output
}

fn finish_with_status(mut owner: PtyChild, timeout: Duration) -> (bool, Vec<u8>) {
    let deadline = Instant::now() + timeout;
    loop {
        match owner.child.try_wait().expect("poll PTY child") {
            Some(status) => {
                let success = status.success();
                drop(owner.input);
                owner.reader.join().expect("join PTY reader");
                let output = owner.output.lock().expect("PTY output lock").clone();
                return (success, output);
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            None => {
                owner.child.kill().expect("kill timed-out PTY owner");
                panic!("PTY owner did not exit within {timeout:?}");
            }
        }
    }
}

fn wait_for_owner(product: &InstalledProduct, session: &str, owner: &mut PtyChild) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if owner_is_ready(product, session) {
            return;
        }
        if let Some(status) = owner.child.try_wait().expect("poll starting owner") {
            panic!(
                "installed owner exited with {status}: {}",
                String::from_utf8_lossy(&owner.output.lock().expect("PTY output lock"))
            );
        }
        assert!(
            Instant::now() < deadline,
            "installed owner did not start: {}",
            String::from_utf8_lossy(&owner.output.lock().expect("PTY output lock"))
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn owner_is_ready(product: &InstalledProduct, session: &str) -> bool {
    let directory = product.state.join("runtime/instances");
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        fs::read(entry.path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|metadata| {
                metadata["session_id"] == session
                    && metadata["control_protocol"].as_u64()
                        == Some(u64::from(proqi::ports::control::CONTROL_PROTOCOL_VERSION))
                    && metadata["control_endpoint"]
                        .as_str()
                        .is_some_and(|endpoint| std::path::Path::new(endpoint).exists())
            })
    })
}

fn json_input(product: &InstalledProduct, arguments: &[&str], input: &str) -> Value {
    let mut command = product.state_command();
    command
        .arg("--json")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = command.spawn().expect("spawn forwarded command");
    child
        .stdin
        .take()
        .expect("forwarded stdin")
        .write_all(input.as_bytes())
        .expect("write forwarded content");
    parse_success(&child.wait_with_output().expect("wait for forwarding"))
}

fn assert_terminal_restored(output: &[u8]) {
    for sequence in [b"\x1b[?1049h".as_slice(), b"\x1b[?1049l".as_slice()] {
        assert!(
            output
                .windows(sequence.len())
                .any(|window| window == sequence),
            "terminal output omitted restoration sequence {sequence:?}: {}",
            String::from_utf8_lossy(output)
        );
    }
}
