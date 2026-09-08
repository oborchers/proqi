//! Real Board f and Edit Control+Shift+F transactions with durable history.

use super::{
    support::{expect_command, json_command},
    watchdog,
};
use std::time::Duration;

const WORKFLOW: &str = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        proc register_watchdog_pid {} {
            set output [open $::env(PROQI_TEST_PIDS) a]
            puts $output [exp_pid]
            close $output
        }
        set stty_init "rows 12 columns 80"
        spawn $binary --state-dir $state -r $env(PROQI_TEST_SESSION)
        register_watchdog_pid
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~board  text\nwrapped  prose\x1b\[201~"
        after 300
        send "\x1b"
        after 100
        send "f"
        after 300
        send "uf"
        after 300
        send "n"
        send -- "\x1b\[200~editor  text\r\nwrapped  prose\x1b\[201~"
        send -- "\x1b\[70;5u"
        after 300
        send -- "\x1b\[122;9u"
        after 200
        send -- "\x1b\[122;10u"
        after 200
        send -- "\x1b\[70;5u\x1b\[70;5u"
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait first
        if {[lindex $first 3] != 0} { exit [lindex $first 3] }
        spawn $binary --state-dir $state -c
        register_watchdog_pid
        expect -exact "\x1b\[?1049h"
        after 200
        send "q"
        expect eof
        catch wait second
        exit [lindex $second 3]
    "#;

#[test]
fn board_and_edit_reflow_chords_commit_exact_restart_safe_content() {
    let state = tempfile::tempdir().expect("isolated state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session");
    let cleanup_pids = state.path().join("reflow-watchdog-pids");
    let mut command = expect_command();
    command
        .args(["-c", WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_PIDS", &cleanup_pids)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        Duration::from_secs(30),
        &cleanup_pids,
        "reflow PTY workflow",
    );
    assert!(status.success(), "reflow PTY exited with {status}");
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 2);
    assert_eq!(thoughts[0]["content"], "board text\nwrapped prose");
    assert_eq!(thoughts[1]["content"], "editor text\r\nwrapped prose");
}
