//! Exact macOS history chords, contextual ownership, and durable Compose handoff.

use super::support::{expect_command, json_command};

const CONTENT: &str = "Compose Grüße 界 e\u{301}\nsecond line";

#[test]
fn compose_and_query_history_use_exact_primary_chords_without_punching_through() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");

    let script = r#"
        log_user 0
        set timeout 12
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"

        # An explicitly resumed empty session starts in Compose. Materialize
        # its first paste as one unit without a printable bootstrap key.
        send -- "\x1b\[200~Compose Grüße 界 e\u0301\nsecond line\x1b\[201~"
        after 500

        # Physical Kitty keyboard encodings for Command+Z and Command+Shift+Z.
        send -- "\x1b\[122;9u"
        send -- "\x1b\[122;9u"
        after 300
        send -- "\x1b\[90;10u"
        after 300
        send -- "\x1b\[122;9u"
        after 300

        # Command+Y is the second registry-owned redo spelling.
        send -- "\x1b\[121;9u"
        after 300

        # Raw Control is an action-specific terminal-safe macOS alias. It is
        # not a second spelling of Primary for unrelated commands.
        send -- "\x1b\[122;5u"
        send -- "\x1b\[122;5u"
        after 300
        send -- "\x1b\[122;6u"
        after 300
        send -- "\x1b\[122;5u"
        after 300
        send -- "\x1b\[121;5u"
        send "\x1b"
        after 200

        # Search owns both presses. The second empty-query undo must not reach
        # the durable Board Create underneath it.
        send "/"
        send -- "absent"
        send -- "\x1b\[122;5u"
        send -- "\x1b\[122;5u"
        send "\x1b"
        after 300
        send "q"
        expect -exact "\x1b\[?1049l"
        expect eof
        catch wait first
        if {[lindex $first 3] != 0} { exit [lindex $first 3] }

        # A separate process proves the restored Create remains restart-safe.
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        after 200
        send "q"
        expect -exact "\x1b\[?1049l"
        expect eof
        catch wait second
        exit [lindex $second 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env_remove("HERDR_ENV")
        .status()
        .expect("run unified history PTY workflow");
    assert!(status.success(), "unified history PTY exited with {status}");

    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0]["content"], CONTENT);
}
