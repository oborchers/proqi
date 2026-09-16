//! Real accepted restart failure followed by exact manual session recovery.

use std::{fs, os::unix::fs::symlink, path::Path, process::Child, time::Duration};

use proqi::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{SystemClock, SystemIdGenerator},
        update::{FileUpdateStateStore, SystemInstallDetector},
    },
    domain::{
        ExternalRestartExpectation, ExternalRestartPending, InstallationIdentity, StableVersion,
        Timestamp,
    },
    ports::{
        environment::{Clock as _, IdGenerator as _},
        update::{
            InstallDetector as _, UpdateParticipantGateway as _, UpdatePrepareRequest,
            UpdateQuiesceRequest, UpdateRestartRequest, UpdateStateStore as _,
        },
    },
};
use rusqlite::Connection;

use super::{active_instances, control_ready, isolated_state, rewrite_versions};
use crate::support::{expect_command, json_command, json_input_command, wait_for_path};

#[test]
fn failed_external_exec_is_recoverable_by_exact_manual_resume() {
    let state = isolated_state("proqi-external-manual-recovery");
    let binary = crate::update_control::fake_homebrew_binary(state.path());
    let active = state.path().join("prefix/opt/proqi/bin/proqi");
    let (session, content) = create_recovery_session(&binary, state.path());
    let failed = fail_accepted_restart(&binary, &active, state.path(), &session);

    assert!(active_instances(state.path()).is_empty());
    let incomplete = failed
        .cache
        .load(failed.installation)
        .expect("pending cache");
    assert!(incomplete.restart_needed);
    assert_eq!(incomplete.external_restart, Some(failed.pending));

    symlink(&binary, &active).expect("restore active executable");
    run_manual_resume(&active, state.path(), &session);
    let recovered = failed
        .cache
        .load(failed.installation)
        .expect("recovered cache");
    assert!(!recovered.restart_needed);
    assert!(recovered.external_restart.is_none());
    assert_recovered_content(&active, state.path(), &session, &content);
}

struct FailedRestart {
    installation: InstallationIdentity,
    pending: ExternalRestartPending,
    cache: FileUpdateStateStore,
}

fn create_recovery_session(binary: &Path, state: &Path) -> (String, String) {
    let created = json_command(binary.to_str().expect("fixture binary UTF-8"), state, &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    let content = "failed exec manual recovery Grüße 界\t\u{1b}[31m\u{7}".to_owned();
    let added = json_input_command(
        binary.to_str().expect("fixture binary UTF-8"),
        state,
        &["thoughts", "add", &session],
        &content,
    );
    assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);
    (session, content)
}

fn fail_accepted_restart(
    binary: &Path,
    active: &Path,
    state: &Path,
    session: &str,
) -> FailedRestart {
    let ready = state.join("failed-exec-owner-ready");
    let mut owner = spawn_failing_owner(binary, state, session, &ready);
    wait_for_path(&ready);
    let mut participant = wait_for_owner(state, &mut owner);
    rewrite_versions(state, "0.9.0");
    "0.9.0".clone_into(&mut participant.version);
    let installation = SystemInstallDetector::for_executable(binary.to_path_buf())
        .detect()
        .expect("Homebrew installation");
    let observed = StableVersion::parse("0.9.0").expect("observed version");
    let current = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("current version");
    let mut ids = SystemIdGenerator;
    let operation_id = ids.request_id();
    let pending = ExternalRestartPending::new(
        current.clone(),
        operation_id,
        vec![ExternalRestartExpectation::new(
            participant.session_id,
            participant.instance_id,
            participant.pid,
            observed.clone(),
        )],
    )
    .expect("pending restart");
    let cache = FileUpdateStateStore::new(&state.join("cache")).expect("update cache");
    cache
        .record_restart_state(installation.identity, observed.clone(), true)
        .expect("stale observation");
    cache
        .reconcile_external_upgrade(installation.identity, &observed, &current, Some(&pending))
        .expect("pending external restart");

    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let prepared = gateway
        .prepare(
            &participant,
            &UpdatePrepareRequest {
                operation_id,
                target_version: current.clone(),
                installation_identity: installation.identity,
                deadline,
            },
        )
        .expect("prepare owner");
    assert!(matches!(
        prepared,
        proqi::ports::update::UpdatePrepareReply::Ready { .. }
    ));
    gateway
        .quiesce(
            &participant,
            &UpdateQuiesceRequest {
                operation_id,
                installed_version: current.clone(),
            },
        )
        .expect("quiesce owner");
    fs::remove_file(active).expect("make active executable unavailable");
    let restart = gateway
        .restart(
            &participant,
            &UpdateRestartRequest {
                operation_id,
                installed_version: current.clone(),
            },
        )
        .expect("accepted restart request");
    assert!(restart.accepted);
    let status = owner.wait().expect("wait for failed exec owner");
    assert!(status.success(), "failed-exec fixture failed: {status}");
    FailedRestart {
        installation: installation.identity,
        pending,
        cache,
    }
}

fn assert_recovered_content(binary: &Path, state: &Path, session: &str, content: &str) {
    let listed = json_command(
        binary.to_str().expect("active binary UTF-8"),
        state,
        &["thoughts", "list", session],
    );
    assert_eq!(listed["data"]["thoughts"][0]["content"], content);
    let integrity: String = Connection::open(state.join("data/proqi.sqlite3"))
        .expect("database")
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .expect("database integrity");
    assert_eq!(integrity, "ok");
}

fn spawn_failing_owner(binary: &Path, state: &Path, session: &str, ready: &Path) -> Child {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        expect -exact "\x1b\[0 q"
        expect -exact "\x1b\[?1049l"
        expect eof
        catch wait result
        if {[lindex $result 3] == 0} { exit 91 }
        exit 0
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .spawn()
        .expect("spawn failing owner")
}

fn wait_for_owner(state: &Path, owner: &mut Child) -> proqi::ports::runtime::InstanceInfo {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let active = active_instances(state);
        if let Some(participant) = active.into_iter().find(control_ready) {
            return participant;
        }
        assert!(
            owner.try_wait().expect("poll owner").is_none(),
            "owner exited before control readiness"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "owner readiness timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_manual_resume(binary: &Path, state: &Path, session: &str) {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        send "q"
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run exact manual resume");
    assert!(status.success(), "manual resume failed: {status}");
}
