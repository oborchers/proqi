//! Terminal shutdown, restoration, and accepted-work durability.

use std::{fs, os::unix::fs::PermissionsExt as _, path::Path};

use super::{expect_command, json_command};

#[test]
fn termination_signal_restores_and_releases_the_session() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let terminate = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        set child [exp_pid]
        expect -exact "\x1b\[?1049h"
        expect -exact "\x1b\[1 q"
        system /bin/kill -TERM $child
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", terminate])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY signal workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "resumable");
}

#[test]
fn hangup_uses_the_operating_system_default_and_never_leaves_a_lease() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let hangup = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        set child [exp_pid]
        expect -exact "\x1b\[?1049h"
        system /bin/kill -HUP $child
        expect eof
        catch wait result
        exit 0
    "#;
    let status = expect_command()
        .args(["-c", hangup])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY hangup workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let resumed = json_command(binary, state.path(), &["-r", session]);
    assert_eq!(resumed["data"]["session_id"], session);
}

#[test]
fn one_hundred_terminal_cycles_restore_and_leave_no_processes() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let cycles = r#"
        log_user 0
        set timeout 10
        for {set cycle 0} {$cycle < 100} {incr cycle} {
            spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
            expect -exact "\x1b\[?1049h"
            send -- "q"
            expect -exact "\x1b\[0 q"
            expect eof
            catch wait result
            if {[lindex $result 3] != 0} { exit 91 }
        }
        exit 0
    "#;
    let status = expect_command()
        .args(["-c", cycles])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run repeated PTY shutdown workflow");
    assert!(status.success());

    let pattern = format!("{binary} --state-dir {}", state.path().display());
    let remaining = std::process::Command::new("/usr/bin/pgrep")
        .args(["-f", &pattern])
        .status()
        .expect("inspect repeated PTY descendants");
    assert!(
        !remaining.success(),
        "a repeated PTY process survived shutdown"
    );
}

#[test]
fn queued_quit_waits_for_the_preceding_paste_to_become_durable() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let quit = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~accepted pending work 界\x1b\[201~"
        send -- "\x11"
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", quit])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY pending-work quit");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "accepted pending work 界"
    );
}

#[test]
fn acknowledged_paste_survives_forced_process_termination() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let crash = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        set child [exp_pid]
        after 500
        send -- "\x1b\[200~committed before crash 界\x1b\[201~"
        after 800
        system /bin/kill -KILL $child
        expect eof
        catch wait result
        exit 0
    "#;
    let status = expect_command()
        .args(["-c", crash])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY crash workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "recovered");
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "committed before crash 界"
    );
}

#[test]
fn keyboard_quit_during_delayed_capture_commit_restores_and_preserves_content() {
    delayed_capture_shutdown(false, false);
}

#[test]
fn termination_during_delayed_capture_commit_restores_and_preserves_content() {
    delayed_capture_shutdown(true, false);
}

#[test]
fn termination_during_persistently_failed_capture_restores_and_exits_boundedly() {
    delayed_capture_shutdown(true, true);
}

fn delayed_capture_shutdown(terminate: bool, persistent_failure: bool) {
    let state = tempfile::Builder::new()
        .prefix("pq-cap-")
        .tempdir_in("/private/tmp")
        .expect("short temporary state");
    let watched = tempfile::tempdir().expect("temporary watched directory");
    let staging = tempfile::tempdir().expect("temporary staging directory");
    configure_capture(state.path(), watched.path());
    let staged = staging.path().join("delayed.png");
    fs::write(&staged, png_bytes()).expect("staged screenshot");
    let target = watched.path().join("delayed.png");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let exit_action = if terminate {
        "system /bin/kill -TERM $child"
    } else {
        "send -- \"\\x11\""
    };
    let finish_action = if persistent_failure {
        r#"
        set spawn_id $proqi
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        set proqi_status [lindex $result 3]
        set spawn_id $sqlite
        send -- "ROLLBACK;\r.quit\r"
        expect eof
        exit $proqi_status
        "#
    } else {
        r#"
        after 150
        set spawn_id $sqlite
        send -- "COMMIT;\r.quit\r"
        expect eof
        set spawn_id $proqi
        expect -exact "\x1b\[0 q"
        expect eof
        catch wait result
        exit [lindex $result 3]
        "#
    };
    let workflow = format!(
        r#"
        log_user 0
        set timeout 15
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        stty rows 24 columns 80
        set proqi $spawn_id
        set child [exp_pid]
        expect -exact "\x1b\[?1049h"
        expect -exact "\x1b\[1 q"
        after 300
        send -- ":"
        after 100
        send -- "Screenshot Inbox"
        send -- "\r"
        set capture_lock "$env(PROQI_TEST_STATE)/runtime/screenshot-capture.json"
        for {{set attempt 0}} {{$attempt < 40 && ![file exists $capture_lock]}} {{incr attempt}} {{
            after 50
        }}
        if {{![file exists $capture_lock]}} {{
            set instances [glob -nocomplain "$env(PROQI_TEST_STATE)/runtime/instances/*.json"]
            if {{[llength $instances] == 1}} {{
                set handle [open [lindex $instances 0] r]
                set metadata [read $handle]
                close $handle
                if {{[string first "\"control_protocol\":null" $metadata] < 0}} {{ exit 94 }}
            }}
            exit 93
        }}
        send -- "\x1b\[200~durable editor\x1b\[201~"
        after 700
        spawn /usr/bin/sqlite3 $env(PROQI_TEST_DATABASE)
        set sqlite $spawn_id
        expect -re "sqlite>"
        send -- "BEGIN IMMEDIATE;\r"
        expect -re "sqlite>"
        file rename $env(PROQI_TEST_STAGED) $env(PROQI_TEST_TARGET)
        set spawn_id $proqi
        after 500
        send -- "!"
        {exit_action}
        {finish_action}
        "#
    );
    let status = expect_command()
        .args(["-c", &workflow])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env(
            "PROQI_TEST_DATABASE",
            state.path().join("data/proqi.sqlite3"),
        )
        .env("PROQI_TEST_STAGED", &staged)
        .env("PROQI_TEST_TARGET", &target)
        .status()
        .expect("run delayed capture shutdown");
    if persistent_failure {
        assert_eq!(
            status.code(),
            Some(1),
            "persistent failure must be truthful"
        );
    } else {
        assert!(status.success(), "delayed capture shutdown exited {status}");
    }

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let contents = thoughts["data"]["thoughts"]
        .as_array()
        .expect("thoughts")
        .iter()
        .map(|thought| thought["content"].as_str().expect("content"))
        .collect::<Vec<_>>();
    let expected = if persistent_failure {
        vec!["durable editor"]
    } else {
        vec!["durable editor!", target.to_str().expect("target")]
    };
    assert_eq!(contents, expected);
}

fn configure_capture(state: &Path, watched: &Path) {
    let config_directory = state.join("config");
    fs::create_dir(&config_directory).expect("config directory");
    let config = config_directory.join("config.toml");
    fs::write(
        &config,
        format!(
            "check_for_updates = false\nkeyboard_enhancement = 'disabled'\n[screenshot_inbox]\ndirectory = '{}'\ncapture_all_new_images = true\ndebounce_ms = 100\n",
            watched.display()
        ),
    )
    .expect("capture config");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).expect("private config");
}

fn png_bytes() -> Vec<u8> {
    let mut bytes = vec![0; 80];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes[12..16].copy_from_slice(b"IHDR");
    bytes[16..20].copy_from_slice(&20_u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&10_u32.to_be_bytes());
    bytes
}
