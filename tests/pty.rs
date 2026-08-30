//! Real process and pseudo-terminal smoke tests.

#[cfg(target_os = "macos")]
#[path = "pty/recovery.rs"]
mod recovery;

#[cfg(target_os = "macos")]
#[path = "pty/reorder.rs"]
mod reorder;

#[cfg(target_os = "macos")]
#[path = "pty/collapsed_entry.rs"]
mod collapsed_entry;

#[cfg(target_os = "macos")]
#[path = "pty/invocation.rs"]
mod invocation;

#[cfg(target_os = "macos")]
#[path = "pty/onboarding.rs"]
mod onboarding;

#[cfg(target_os = "macos")]
#[path = "pty/support.rs"]
mod support;

#[cfg(target_os = "macos")]
use support::{
    consume_first_run, expect_command, json_command, json_input_command, raw_input_command,
    wait_for_control_owner, wait_for_path,
};

#[path = "pty/smoke.rs"]
mod smoke;

#[cfg(target_os = "macos")]
#[test]
fn bracketed_paste_autosaves_and_resumes_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let create = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
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
        expect -exact "\x1b\[?1049h"
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
    consume_first_run(binary, state.path());
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
        expect -exact "\x1b\[?1049h"
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
    consume_first_run(binary, state.path());
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
fn shifted_arrow_range_selection_deletes_one_real_pty_block() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[200~first\x1b\[201~"
        after 150
        send "\x1b"
        send -- "\x1b\[200~Grüße 👩‍💻\x1b\[201~"
        after 150
        send "\x1b"
        send -- "\x1b\[200~第三\x1b\[201~"
        after 300
        send "\x1b"
        send -- "\x1b\[1;2A\x1b\[1;2A"
        after 100
        send "d"
        after 500
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
        .expect("run PTY range-selection workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"].as_array().map(Vec::len),
        Some(0)
    );
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
        expect -exact "\x1b\[?1049h"
        send -- "Needle"
        expect -re "Needle"
        send "\r"
        expect -exact "\x1b\[?1049l"; expect -exact "\x1b\[?1049h"
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
#[path = "pty/watchdog.rs"]
mod watchdog;

#[cfg(target_os = "macos")]
#[path = "pty/update_control.rs"]
mod update_control;

#[path = "pty/select_all.rs"]
mod select_all;

#[cfg(target_os = "macos")]
#[path = "pty/smart_lists.rs"]
mod smart_lists;
