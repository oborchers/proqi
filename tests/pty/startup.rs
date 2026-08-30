//! Startup input admission and resize ordering after terminal ownership.

use super::support::{expect_command, json_command};

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
