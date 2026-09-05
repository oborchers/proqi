//! Real owner-control update readiness with an injected installer.

use std::{fs, path::Path, process::Child};

use std::os::unix::fs::symlink;

use proqi::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        update::{FileUpdateStateStore, SystemInstallDetector},
    },
    application::UpdateRestartCoordinator,
    domain::{InstallationIdentity, StableVersion, Timestamp},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::InstanceInfo,
        update::{
            HomebrewInstaller, InstallDetector as _, UPDATE_CONTROL_PROTOCOL_VERSION, UpdateError,
            UpdateParticipantGateway as _, UpdatePrepareReply, UpdatePrepareRequest,
            UpdateRestartRequest, UpdateStateStore as _,
        },
    },
};

use super::support::{
    expect_command, json_command, json_input_command, wait_for_control_owner, wait_for_path,
};

#[path = "update_control/highlight_fixture.rs"]
mod highlight_fixture;

struct FakeInstaller {
    calls: usize,
}

impl HomebrewInstaller for FakeInstaller {
    fn upgrade(&mut self, expected: &StableVersion) -> Result<StableVersion, UpdateError> {
        self.calls = self.calls.saturating_add(1);
        Ok(expected.clone())
    }
}

#[test]
fn real_owner_preflights_and_returns_to_use_after_one_fake_installation() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("update-owner-ready");
    let done = state.path().join("update-owner-done");
    let mut owner = spawn_owner(binary, state.path(), session, &ready, &done);
    wait_for_path(&ready);
    let participant = wait_for_control_owner(state.path(), session);
    let installation = SystemInstallDetector::for_executable(binary.into())
        .detect()
        .expect("installation identity");
    let mut ids = SystemIdGenerator;
    let registry = FileRuntimeCoordinator::new(
        state.path().join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("registry")
    .with_update_context(installation.identity, UPDATE_CONTROL_PROTOCOL_VERSION);
    let update_state = FileUpdateStateStore::new(&state.path().join("cache")).expect("state");
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let mut installer = FakeInstaller { calls: 0 };
    let target = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("version");
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));

    let result =
        UpdateRestartCoordinator::new(&update_state, &registry, &mut gateway, &mut installer)
            .execute(
                ids.request_id(),
                participant.instance_id,
                installation.identity,
                &target,
                deadline,
                &(),
            )
            .expect("coordinate update");

    assert_eq!(result.prepared_participants, 1, "{result:?}");
    assert_eq!(result.restart_requests, 0);
    assert!(result.restart_failed.is_empty());
    assert_eq!(installer.calls, 1);
    assert_eq!(participant.session_id.to_string(), session);
    let added = json_input_command(
        binary,
        state.path(),
        &["thoughts", "add", session],
        "owner remained usable after update release",
    );
    assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);

    fs::write(&done, b"done").expect("release owner");
    let status = owner.wait().expect("wait for owner");
    assert!(status.success(), "owner PTY exited with {status}");
}

#[test]
fn homebrew_owner_restores_and_replaces_itself_in_the_same_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = fake_homebrew_binary(state.path());
    let original = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(original, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("exec-owner-ready");
    let restarted = state.path().join("exec-owner-restarted");
    let done = state.path().join("exec-owner-done");
    let mut owner =
        spawn_restarting_owner(&binary, state.path(), session, &ready, &restarted, &done);
    wait_for_path(&ready);
    let before = wait_for_control_owner(state.path(), session);
    let installation = SystemInstallDetector::for_executable(binary.clone())
        .detect()
        .expect("Homebrew installation");
    let version = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("version");
    let mut ids = SystemIdGenerator;
    let operation_id = ids.request_id();
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let reply = gateway
        .prepare(
            &before,
            &UpdatePrepareRequest {
                operation_id,
                target_version: version.clone(),
                installation_identity: installation.identity,
                deadline,
            },
        )
        .expect("prepare owner");
    assert!(matches!(reply, UpdatePrepareReply::Ready { .. }));
    let restart = gateway
        .restart(
            &before,
            &UpdateRestartRequest {
                operation_id,
                installed_version: version,
            },
        )
        .expect("restart owner");
    assert!(restart.accepted);

    wait_for_path(&restarted);
    let after = wait_for_control_owner(state.path(), session);
    assert_eq!(
        after.pid, before.pid,
        "Unix exec must preserve the process ID"
    );
    assert_ne!(after.instance_id, before.instance_id);
    fs::write(&done, b"done").expect("release restarted owner");
    let status = owner.wait().expect("wait for restarted owner");
    assert!(status.success(), "restarted owner PTY exited with {status}");
}

#[test]
fn in_app_restart_reopens_until_dismissed_then_remains_manual() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = fake_homebrew_binary(state.path());
    let original = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(original, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let peer_created = json_command(original, state.path(), &[]);
    let peer_session = peer_created["data"]["session_id"]
        .as_str()
        .expect("peer session ID");
    let peer_ready = state.path().join("highlight-peer-ready");
    let peer_restarted = state.path().join("highlight-peer-restarted");
    let peer_done = state.path().join("highlight-peer-done");
    let mut peer = highlight_fixture::spawn_quiet_restarting_peer(
        &binary,
        state.path(),
        peer_session,
        &peer_ready,
        &peer_restarted,
        &peer_done,
    );
    wait_for_path(&peer_ready);
    let mut peer_participant = wait_for_control_owner(state.path(), peer_session);
    let ready = state.path().join("highlight-owner-ready");
    let mut owner = highlight_fixture::spawn_crash_owner(&binary, state.path(), session, &ready);
    wait_for_path(&ready);
    let mut before = wait_for_control_owner(state.path(), session);
    rewrite_participant_version(state.path(), &mut before, "0.3.0");
    rewrite_participant_version(state.path(), &mut peer_participant, "0.3.0");
    let (update_state, installation, target) =
        coordinate_highlight_restart(&binary, state.path(), &before);
    wait_for_path(&peer_restarted);
    fs::write(&peer_done, b"done").expect("release restarted peer");
    let peer_status = peer.wait().expect("wait for restarted peer");
    assert!(
        peer_status.success(),
        "restarted peer exited with {peer_status}"
    );
    let peer_thoughts = json_command(original, state.path(), &["thoughts", "list", peer_session]);
    assert_eq!(
        peer_thoughts["data"]["thoughts"][0]["content"],
        highlight_fixture::PEER_QUIET_PROOF
    );
    let status = owner.wait().expect("wait for crash fixture");
    assert!(status.success(), "crash fixture exited with {status}");
    let pending = update_state
        .load(installation)
        .expect("pending state")
        .release_highlights
        .expect("pending announcement");
    assert!(!pending.acknowledged());

    let status = highlight_fixture::run_dismissal(&binary, state.path(), session);
    assert!(status.success(), "dismissal fixture exited with {status}");
    let acknowledged = update_state
        .load(installation)
        .expect("acknowledged state")
        .release_highlights
        .expect("acknowledged announcement");
    assert!(acknowledged.acknowledged());

    let status = highlight_fixture::run_quiet_resume(&binary, state.path(), session);
    assert!(
        status.success(),
        "quiet resume fixture exited with {status}"
    );
    let resumed = json_command(original, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        resumed["data"]["thoughts"][0]["content"],
        highlight_fixture::RESUME_QUIET_PROOF
    );
    let status = highlight_fixture::run_manual_reopen(&binary, state.path(), session);
    assert!(
        status.success(),
        "manual reopen fixture exited with {status}"
    );
    assert_eq!(target.to_string(), env!("CARGO_PKG_VERSION"));
}

fn coordinate_highlight_restart(
    binary: &Path,
    state: &Path,
    before: &InstanceInfo,
) -> (FileUpdateStateStore, InstallationIdentity, StableVersion) {
    let installation = SystemInstallDetector::for_executable(binary.to_path_buf())
        .detect()
        .expect("Homebrew installation");
    let mut ids = SystemIdGenerator;
    let registry = FileRuntimeCoordinator::new(
        state.join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("registry")
    .with_update_context(installation.identity, UPDATE_CONTROL_PROTOCOL_VERSION);
    let update_state = FileUpdateStateStore::new(&state.join("cache")).expect("state");
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let mut installer = FakeInstaller { calls: 0 };
    let target = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("target");
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));

    let result =
        UpdateRestartCoordinator::new(&update_state, &registry, &mut gateway, &mut installer)
            .execute(
                ids.request_id(),
                before.instance_id,
                installation.identity,
                &target,
                deadline,
                &(),
            )
            .expect("coordinate restart");
    assert_eq!(result.restart_requests, 2);
    assert!(result.restart_failed.is_empty());
    let recorded = update_state
        .load(installation.identity)
        .expect("recorded state")
        .release_highlights
        .expect("recorded announcement");
    assert_eq!(recorded.session_id(), before.session_id);
    let manifest = proqi::adapters::update::packaged_release_highlights().expect("manifest");
    let selected = proqi::application::ReleaseHighlightSelection::select(
        &manifest,
        &update_state
            .load(installation.identity)
            .expect("selection state"),
        before.session_id,
        &target,
    );
    assert!(selected.automatic.is_some());
    (update_state, installation.identity, target)
}

fn rewrite_participant_version(state: &Path, participant: &mut InstanceInfo, version: &str) {
    version.clone_into(&mut participant.version);
    let path = state
        .join("runtime/instances")
        .join(format!("{}.json", participant.instance_id));
    fs::write(
        path,
        serde_json::to_vec(participant).expect("serialize participant"),
    )
    .expect("rewrite participant version");
}

fn spawn_owner(binary: &str, state: &Path, session: &str, ready: &Path, done: &Path) -> Child {
    let script = r#"
        log_user 0
        set timeout 15
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        set deadline [expr {[clock milliseconds] + 15000}]
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 91 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 93 }
            }
            after 20
        }
        send -- $env(PROQI_TEST_PRIMARY_Q)
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_DONE", done)
        .spawn()
        .expect("spawn update owner")
}

fn fake_homebrew_binary(root: &Path) -> std::path::PathBuf {
    let keg = root.join(format!("prefix/Cellar/proqi/{}", env!("CARGO_PKG_VERSION")));
    let binary = keg.join("bin/proqi");
    fs::create_dir_all(binary.parent().expect("binary parent")).expect("create fake keg");
    fs::copy(env!("CARGO_BIN_EXE_proqi"), &binary).expect("copy test binary");
    fs::write(keg.join("INSTALL_RECEIPT.json"), b"{}").expect("write receipt");
    let active = root.join("prefix/opt/proqi/bin/proqi");
    fs::create_dir_all(active.parent().expect("active parent")).expect("create active path");
    symlink(&binary, active).expect("link active binary");
    binary
}

fn spawn_restarting_owner(
    binary: &Path,
    state: &Path,
    session: &str,
    ready: &Path,
    restarted: &Path,
    done: &Path,
) -> Child {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        expect -exact "\x1b\[0 q"
        expect -exact "\x1b\[?1049l"
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_RESTARTED) w]
        set deadline [expr {[clock milliseconds] + 15000}]
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 92 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 93 }
            }
            after 20
        }
        send -- $env(PROQI_TEST_PRIMARY_Q)
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_RESTARTED", restarted)
        .env("PROQI_TEST_DONE", done)
        .spawn()
        .expect("spawn restarting owner")
}
