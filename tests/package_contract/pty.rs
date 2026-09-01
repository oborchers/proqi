//! Safe Unix PTY coverage for the copied release executable.

use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

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

use super::{InstalledProduct, package_version};

#[path = "pty/process.rs"]
mod process;

use process::{PtyChild, assert_terminal_restored};

pub(super) fn assert_active_owner_and_terminal_restoration(
    product: &InstalledProduct,
    session: &str,
) {
    let mut owner = PtyChild::spawn(product, session);
    owner.wait_for_owner(product, session);
    let forwarded = product.json_input(
        &["thoughts", "add", session],
        "forwarded through the installed owner Grüße 界",
    );
    assert!(
        forwarded["data"]["thought_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("tht_"))
    );
    owner.input().write_all(b"q").expect("quit owner");
    owner.input().flush().expect("flush quit");
    let output = owner.finish(Duration::from_secs(10));
    assert_terminal_restored(&output);

    let created = product.json(&[]);
    let terminated_session = created["data"]["session_id"]
        .as_str()
        .expect("termination session");
    let mut owner = PtyChild::spawn(product, terminated_session);
    owner.wait_for_owner(product, terminated_session);
    let process_id = owner.process_id();
    let termination = Command::new("/bin/kill")
        .args(["-TERM", &process_id.to_string()])
        .status()
        .expect("signal PTY owner");
    assert!(termination.success());
    let output = owner.finish(Duration::from_secs(10));
    assert_terminal_restored(&output);
    let resumed = product.json(&["-r", terminated_session]);
    assert_eq!(resumed["data"]["session_id"], terminated_session);
    assert_drop_guard_releases_owner(product);
    assert_update_contract(product);
}

fn assert_drop_guard_releases_owner(product: &InstalledProduct) {
    let created = product.json(&[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("drop-guard session");
    let mut owner = PtyChild::spawn(product, session);
    owner.wait_for_owner(product, session);
    let process_id = owner.process_id();
    drop(owner);
    let probe = Command::new("/bin/kill")
        .args(["-0", &process_id.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("probe dropped PTY owner");
    assert!(!probe.success(), "dropped PTY owner remained alive");
    let resumed = product.json(&["-r", session]);
    assert_eq!(resumed["data"]["session_id"], session);
    let instances = product.state.join("runtime/instances");
    let survivors = runtime_instance_summaries(&instances);
    assert!(
        survivors.is_empty(),
        "dropped owner {session} pid {process_id} metadata survived recovery: {survivors:?}"
    );
}

fn runtime_instance_summaries(
    instances: &std::path::Path,
) -> Vec<(String, String, String, u64, u64)> {
    fs::read_dir(instances)
        .expect("read cleaned runtime metadata")
        .map(|entry| {
            let entry = entry.expect("runtime metadata entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = fs::read(entry.path()).expect("read runtime metadata entry");
            let metadata = serde_json::from_slice::<serde_json::Value>(&bytes)
                .unwrap_or(serde_json::Value::Null);
            (
                name,
                metadata["instance_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                metadata["session_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                metadata["pid"].as_u64().unwrap_or_default(),
                metadata["control_protocol"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
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
            version: package_version(),
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
    let mut owner = PtyChild::spawn(&homebrew, session);
    owner.wait_for_owner(&homebrew, session);
    let installation = SystemInstallDetector::for_executable(homebrew.binary.clone())
        .detect()
        .expect("fake Homebrew installation");
    assert_fake_update_services(&homebrew, session, installation.identity);
    let registry = update_registry(&homebrew, installation.identity);
    let version = package_version();
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
    owner.input().write_all(b"q").expect("quit replaced owner");
    owner.input().flush().expect("flush replaced quit");
    assert_terminal_restored(&owner.finish(Duration::from_secs(10)));
    assert_failed_exec_is_recoverable(product);
}

fn assert_fake_update_services(
    homebrew: &InstalledProduct,
    session: &str,
    installation: InstallationIdentity,
) {
    let state = FileUpdateStateStore::new(&homebrew.state.join("cache")).expect("update state");
    let version = package_version();
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
    let initiating = active_participant(&registry, session)
        .expect("active package participant")
        .instance_id;
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let mut ids = SystemIdGenerator;
    let mut failing = FakeInstaller {
        fail: true,
        calls: 0,
    };
    let failure = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut failing)
        .execute(
            ids.request_id(),
            initiating,
            installation,
            &version,
            deadline,
            &(),
        );
    assert!(matches!(failure, Err(UpdateError::InstallerFailed)));
    assert_eq!(failing.calls, 1);
    let usable = homebrew.json_input(
        &["thoughts", "add", session],
        "installer failure kept this session usable",
    );
    assert_eq!(usable["data"]["receipt"]["idempotent_replay"], false);
    let mut succeeding = FakeInstaller {
        fail: false,
        calls: 0,
    };
    let installed = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut succeeding)
        .execute(
            ids.request_id(),
            initiating,
            installation,
            &version,
            deadline,
            &(),
        )
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
    let mut owner = PtyChild::spawn(&homebrew, session);
    owner.wait_for_owner(&homebrew, session);
    let installation = SystemInstallDetector::for_executable(homebrew.binary.clone())
        .detect()
        .expect("failed-exec installation");
    let registry = update_registry(&homebrew, installation.identity);
    let before = active_participant(&registry, session).expect("active package participant");
    let mut ids = SystemIdGenerator;
    let operation = ids.request_id();
    let version = package_version();
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
    let (success, output) = owner.finish_with_status(Duration::from_secs(10));
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
    let keg = prefix.join(format!("Cellar/proqi/{}", env!("CARGO_PKG_VERSION")));
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
        sandbox: Arc::clone(&product.sandbox),
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
        env!("CARGO_PKG_VERSION"),
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
