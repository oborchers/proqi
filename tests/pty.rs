//! Real process and pseudo-terminal smoke tests.

use std::process::Command;

#[cfg(target_os = "macos")]
#[path = "pty/recovery.rs"]
mod recovery;

#[cfg(target_os = "macos")]
use serde_json::Value;

#[test]
fn release_entrypoint_can_start_without_workspace_state() {
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--version")
        .output()
        .expect("run proqi binary");

    assert!(output.status.success());
}

#[cfg(target_os = "macos")]
#[test]
fn bracketed_paste_autosaves_and_resumes_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let create = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        after 500
        send -- "\x1b\[200~Grüße 界\nsecond\x1b\[201~"
        after 700
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", create])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY create workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "Grüße 界\nsecond"
    );
    let thought = thoughts["data"]["thoughts"][0]["id"]
        .as_str()
        .expect("thought ID");

    let resume = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        after 500
        send "\r"
        after 100
        send "!"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", resume])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run PTY resume workflow");
    assert!(status.success());

    let inspected = json_command(
        binary,
        state.path(),
        &["thoughts", "inspect", session, thought],
    );
    assert_eq!(inspected["data"]["thought"]["content"], "Grüße 界\nsecond!");

    assert_persistent_editor_undo(binary, state.path(), session, thought);
}

#[cfg(target_os = "macos")]
#[test]
fn startup_typeahead_after_terminal_ownership_is_not_lost() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let startup = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~startup-typeahead\x1b\[201~"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", startup])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY startup typeahead workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "startup-typeahead"
    );
}

#[cfg(target_os = "macos")]
fn assert_persistent_editor_undo(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    thought: &str,
) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        after 500
        send "\r"
        after 100
        send "\x1a"
        after 500
        send "\x1b"
        after 100
        send "q"
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
        .expect("run PTY persistent undo workflow");
    assert!(status.success());
    let undone = json_command(binary, state, &["thoughts", "inspect", session, thought]);
    assert_eq!(undone["data"]["thought"]["content"], "Grüße 界\nsecond");
}

#[cfg(target_os = "macos")]
#[test]
fn keyboard_creation_survives_rapid_pty_resize() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let interact = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "nmouse-created"
        stty rows 4 columns 12
        after 100
        stty rows 30 columns 100
        after 100
        stty rows 6 columns 20
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", interact])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY keyboard and resize workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(thoughts["data"]["thoughts"][0]["content"], "mouse-created");
}

#[cfg(target_os = "macos")]
#[test]
fn session_browser_searches_and_resumes_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let first = json_command(binary, state.path(), &[]);
    let first_id = first["data"]["session_id"].as_str().expect("first ID");
    let _renamed = json_command(
        binary,
        state.path(),
        &["sessions", "rename", first_id, "Other session"],
    );
    let target = json_command(binary, state.path(), &[]);
    let target_id = target["data"]["session_id"].as_str().expect("target ID");
    let _renamed = json_command(
        binary,
        state.path(),
        &["sessions", "rename", target_id, "Needle target"],
    );
    let before = json_command(binary, state.path(), &["sessions", "list"]);
    let opened_before = before["data"]["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|session| session["id"] == target_id)
        .and_then(|session| session["last_opened_at"].as_i64())
        .expect("opening timestamp");

    let browse = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state -r
        stty rows 24 columns 100
        after 500
        send -- "Needle"
        after 200
        send "\r"
        after 500
        send -- "\x11"
        expect {
            eof {}
            timeout { exit 93 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", browse])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY browser workflow");
    assert!(status.success(), "browser PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let selected = sessions["data"]["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|session| session["id"] == target_id)
        .expect("resumed target");
    assert!(
        selected["last_opened_at"]
            .as_i64()
            .is_some_and(|opened_after| opened_after > opened_before)
    );
}

#[cfg(target_os = "macos")]
#[path = "pty/active_control.rs"]
mod active_control;

#[cfg(target_os = "macos")]
#[path = "pty/path_drop.rs"]
mod path_drop;

#[cfg(target_os = "macos")]
#[path = "pty/key_inspector.rs"]
mod key_inspector;

#[cfg(target_os = "macos")]
#[path = "pty/fairness.rs"]
mod fairness;

#[cfg(target_os = "macos")]
#[path = "pty/shutdown.rs"]
mod shutdown;

#[cfg(target_os = "macos")]
#[path = "pty/update_control.rs"]
mod update_control;

#[cfg(target_os = "macos")]
fn expect_command() -> Command {
    let mut command = Command::new("/usr/bin/expect");
    command.env("PROQI_DISABLE_HERDR", "1");
    command
}

#[cfg(target_os = "macos")]
fn wait_for_path(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(target_os = "macos")]
fn wait_for_control_owner(state: &std::path::Path, session: &str) {
    let instances = state.join("runtime/instances");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if control_owner_is_ready(&instances, session) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not advertise a ready control endpoint"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(target_os = "macos")]
fn control_owner_is_ready(instances: &std::path::Path, session: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(instances) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|entry| control_metadata_is_ready(&entry.path(), session))
}

#[cfg(target_os = "macos")]
fn control_metadata_is_ready(path: &std::path::Path, session: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    value["session_id"] == session
        && value["control_protocol"].as_u64() == Some(4)
        && value["control_endpoint"]
            .as_str()
            .is_some_and(|endpoint| std::path::Path::new(endpoint).exists())
}

#[cfg(target_os = "macos")]
fn raw_input_command(
    binary: &str,
    state: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> std::process::Output {
    use std::{io::Write as _, process::Stdio};

    let mut child = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn input command");
    child
        .stdin
        .take()
        .expect("command stdin")
        .write_all(input.as_bytes())
        .expect("write command input");
    child.wait_with_output().expect("wait for input command")
}

#[cfg(target_os = "macos")]
fn json_input_command(
    binary: &str,
    state: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> Value {
    let output = raw_input_command(binary, state, arguments, input);
    assert!(
        output.status.success(),
        "input command failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("input command JSON")
}

#[cfg(target_os = "macos")]
fn json_command(binary: &str, state: &std::path::Path, arguments: &[&str]) -> Value {
    let output = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("run JSON command");
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("JSON output")
}
