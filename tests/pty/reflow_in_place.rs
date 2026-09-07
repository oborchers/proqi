//! Real Board f and Edit Primary+F transactions with restart and durable history.

use super::support::{consume_first_run, expect_command, json_command};

#[test]
fn board_and_edit_reflow_chords_commit_exact_restart_safe_content() {
    let state = tempfile::tempdir().expect("isolated state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set stty_init "rows 12 columns 80"
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~board\nwrapped prose\x1b\[201~"
        after 300
        send "\x1b"
        after 100
        send "f"
        after 300
        send "uf"
        after 300
        send "n"
        send -- "\x1b\[200~editor\r\nwrapped prose\x1b\[201~"
        send -- "\x1b\[102;9u"
        after 300
        send -- "\x1b\[122;9u"
        after 200
        send -- "\x1b\[122;10u"
        after 200
        send -- "\x1b\[102;9u\x1b\[102;9u"
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait first
        if {[lindex $first 3] != 0} { exit [lindex $first 3] }
        spawn $binary --state-dir $state -c
        expect -exact "\x1b\[?1049h"
        after 200
        send "q"
        expect eof
        catch wait second
        exit [lindex $second 3]
    "#;
    let output = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .output()
        .expect("PTY scenario");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 2);
    assert_eq!(thoughts[0]["content"], "board wrapped prose");
    assert_eq!(thoughts[1]["content"], "editor wrapped prose");
}
