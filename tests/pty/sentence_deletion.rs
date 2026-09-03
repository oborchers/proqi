//! Sentence deletion, enhanced-key input, durability, and history in a real PTY.

use super::support::{consume_first_run, expect_command, json_command};

#[test]
fn primary_shift_u_deletes_and_persists_a_sentence_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[200~The quick brown fox\njumped over the hoop. It failed.\x1b\[201~"
        after 200
        send -- "\x1b\[117;10u"
        after 200
        send -- "\x1b\[122;9u"
        after 200
        send -- "\x1b\[90;10u"
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
        .status()
        .expect("run sentence deletion PTY workflow");
    assert!(status.success(), "sentence PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "The quick brown fox\njumped over the hoop."
    );
    let thought = thoughts["data"]["thoughts"][0]["id"]
        .as_str()
        .expect("thought ID");

    let _undo = json_command(
        binary,
        state.path(),
        &["thoughts", "undo", session, "--thought", thought],
    );
    let undone = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        undone["data"]["thoughts"][0]["content"],
        "The quick brown fox\njumped over the hoop. It failed."
    );
    let _redo = json_command(
        binary,
        state.path(),
        &["thoughts", "redo", session, "--thought", thought],
    );
    let redone = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        redone["data"]["thoughts"][0]["content"],
        "The quick brown fox\njumped over the hoop."
    );
}
