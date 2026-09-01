//! Real terminal and SQLite coverage for contextual transformation shortcuts.

use super::support::{expect_command, json_command};

#[test]
fn primary_split_and_plain_board_merge_survive_a_real_pty_restart() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set stty_init "rows 12 columns 100"
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~left right\x1b\[201~"
        after 300
        send "\x1b"
        after 100
        send "\r"
        after 100
        send -- "\x1b\[D\x1b\[D\x1b\[D\x1b\[D\x1b\[D\x1b\[D"
        send -- "\x1b\[116;9u"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait first
        if {[lindex $first 3] != 0} { exit [lindex $first 3] }

        spawn $binary --state-dir $state -c
        expect -exact "\x1b\[?1049h"
        after 300
        send "a"
        send "t"
        after 500
        send "q"
        expect eof
        catch wait second
        exit [lindex $second 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run transformation shortcuts in PTY");
    assert!(status.success(), "transformation PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0]["content"], "left\n\n right");
}
