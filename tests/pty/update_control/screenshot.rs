//! Screenshot Inbox exclusion from update preparation.

use std::os::unix::fs::PermissionsExt as _;

use super::*;

#[test]
fn live_screenshot_watcher_rejects_prepare_before_the_update_barrier() {
    let state = tempfile::tempdir().expect("temporary state");
    let watched = state.path().join("watched");
    fs::create_dir(&watched).expect("watched directory");
    configure_capture(state.path(), &watched);
    let binary = fake_homebrew_binary(state.path());
    let original = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(original, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("screenshot-owner-ready");
    let done = state.path().join("screenshot-owner-done");
    let mut owner = spawn_listening_owner(&binary, state.path(), session, &ready, &done);
    wait_for_path(&ready);
    let participant = wait_for_control_owner(state.path(), session);
    let installation = SystemInstallDetector::for_executable(binary)
        .detect()
        .expect("Homebrew installation");
    let mut ids = SystemIdGenerator;
    let version = StableVersion::parse(env!("CARGO_PKG_VERSION")).expect("version");
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(10_000));
    let reply = LocalUpdateControlClient::new(SystemIdGenerator)
        .prepare(
            &participant,
            &UpdatePrepareRequest {
                operation_id: ids.request_id(),
                target_version: version,
                installation_identity: installation.identity,
                deadline,
            },
        )
        .expect("prepare response");
    assert_eq!(
        reply,
        UpdatePrepareReply::Blocked {
            instance_id: participant.instance_id,
            code: "screenshot_not_quiescent".to_owned(),
        }
    );
    let added = json_input_command(
        original,
        state.path(),
        &["thoughts", "add", session],
        "preparation rejection preserved ordinary use",
    );
    assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);

    fs::write(&done, b"done").expect("release owner");
    let status = owner.wait().expect("wait for owner");
    assert!(status.success(), "owner PTY exited with {status}");
}

fn configure_capture(state: &Path, watched: &Path) {
    let config_directory = state.join("config");
    fs::create_dir(&config_directory).expect("config directory");
    let config = config_directory.join("config.toml");
    fs::write(
        &config,
        format!(
            "check_for_updates = false\nkeyboard_enhancement = 'disabled'\n[screenshot_inbox]\ndirectory = '{}'\ncapture_all_new_images = true\ndebounce_ms = 100\n",
            watched.display(),
        ),
    )
    .expect("capture config");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).expect("private config");
}

fn spawn_listening_owner(
    binary: &Path,
    state: &Path,
    session: &str,
    ready: &Path,
    done: &Path,
) -> Child {
    let script = r#"
        log_user 0
        set timeout 20
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 1000
        send -- "\x1b"
        after 50
        send -- "i"
        set capture_lock "$env(PROQI_TEST_STATE)/runtime/screenshot-capture.json"
        for {set attempt 0} {$attempt < 100 && ![file exists $capture_lock]} {incr attempt} {
            after 50
        }
        if {![file exists $capture_lock]} { exit 94 }
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
        .expect("spawn screenshot update owner")
}
