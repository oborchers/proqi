//! Real process and pseudo-terminal smoke tests.

use std::process::Command;

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
    let status = Command::new("/usr/bin/expect")
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
    let status = Command::new("/usr/bin/expect")
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
    let status = Command::new("/usr/bin/expect")
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
fn termination_signal_restores_and_releases_the_session() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let terminate = r"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        set child [exp_pid]
        after 500
        system /bin/kill -TERM $child
        expect eof
        catch wait result
        exit [lindex $result 3]
    ";
    let status = Command::new("/usr/bin/expect")
        .args(["-c", terminate])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY signal workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "resumable");
}

#[cfg(target_os = "macos")]
#[test]
fn acknowledged_paste_survives_forced_process_termination() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let crash = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        set child [exp_pid]
        after 500
        send -- "\x1b\[200~committed before crash 界\x1b\[201~"
        after 800
        system /bin/kill -KILL $child
        expect eof
        catch wait result
        exit 0
    "#;
    let status = Command::new("/usr/bin/expect")
        .args(["-c", crash])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY crash workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "resumable");
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "committed before crash 界"
    );
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
        send -- "n"
        after 100
        send -- "mouse-created"
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
    let status = Command::new("/usr/bin/expect")
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
