//! Standalone installation replacement through the production owner-control protocol.

use super::*;

#[test]
fn owner_restores_and_replaces_itself_in_the_same_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = fake_binary(state.path());
    let original = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(original, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("standalone-owner-ready");
    let restarted = state.path().join("standalone-owner-restarted");
    let done = state.path().join("standalone-owner-done");
    let mut owner =
        spawn_restarting_owner(&binary, state.path(), session, &ready, &restarted, &done);
    wait_for_path(&ready);
    let before = wait_for_control_owner(state.path(), session);
    let installation = SystemInstallDetector::for_executable(binary.clone())
        .detect()
        .expect("standalone installation");
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
        .expect("prepare standalone owner");
    assert!(matches!(reply, UpdatePrepareReply::Ready { .. }));
    let quiesced = gateway
        .quiesce(
            &before,
            &UpdateQuiesceRequest {
                operation_id,
                installed_version: version.clone(),
            },
        )
        .expect("quiesce standalone owner");
    assert_eq!(quiesced.session_id, before.session_id);
    let verifier = FileRuntimeCoordinator::new(
        state.path().join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("schema verifier");
    let exclusive = verifier
        .acquire_schema_exclusive()
        .expect("standalone quiescence proves shared schema lease release");
    drop(exclusive);
    let restart = gateway
        .restart(
            &before,
            &UpdateRestartRequest {
                operation_id,
                installed_version: version,
            },
        )
        .expect("restart standalone owner");
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

fn fake_binary(root: &Path) -> std::path::PathBuf {
    let directory = root.join("standalone/bin");
    let binary = directory.join("proqi");
    fs::create_dir_all(&directory).expect("create standalone directory");
    fs::copy(env!("CARGO_BIN_EXE_proqi"), &binary).expect("copy test binary");
    fs::write(
        directory.join("proqi-installation.json"),
        br#"{"schema_version":1,"product":"proqi","kind":"standalone_archive"}"#,
    )
    .expect("write standalone marker");
    binary
}
