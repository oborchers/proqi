//! Exact macOS visual-row selection bytes with a durable replacement oracle.

use super::support::{consume_first_run, expect_command, json_command};

#[test]
fn macos_primary_shift_right_replaces_more_than_one_grapheme_in_a_wrapped_row() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let original = "a".repeat(100);
    let script = r#"
        log_user 0
        set timeout 10
        set stty_init "rows 10 columns 24"
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~$env(PROQI_TEST_CONTENT)\x1b\[201~"
        after 200
        send -- "\x1b\[H"
        for {set index 0} {$index < 5} {incr index} {
            send -- "\x1b\[C"
        }
        send -- "\x1b\[1;10C"
        send -- "X"
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
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_CONTENT", &original)
        .status()
        .expect("run visual-row PTY workflow");
    assert!(status.success(), "visual-row PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let persisted = thoughts["data"]["thoughts"][0]["content"]
        .as_str()
        .expect("persisted content");
    assert_eq!(persisted, format!("aaaaaX{}", "a".repeat(78)));
}

#[test]
fn macos_primary_left_moves_without_selection_to_the_current_wrapped_row_start() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let original = "a".repeat(100);
    let script = r#"
        log_user 0
        set timeout 10
        set stty_init "rows 10 columns 24"
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~$env(PROQI_TEST_CONTENT)\x1b\[201~"
        after 200
        send -- "\x1b\[H"
        for {set index 0} {$index < 5} {incr index} {
            send -- "\x1b\[C"
        }
        send -- "\x1b\[1;9D"
        send -- "X"
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
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_CONTENT", &original)
        .status()
        .expect("run visual-row movement PTY workflow");
    assert!(
        status.success(),
        "visual-row movement PTY exited with {status}"
    );

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let persisted = thoughts["data"]["thoughts"][0]["content"]
        .as_str()
        .expect("persisted content");
    assert_eq!(persisted, format!("X{original}"));
}
