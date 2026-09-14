//! Exact macOS visual-row selection bytes with a durable replacement oracle.

use super::support::{consume_first_run, expect_command, json_command, json_input_command};

fn seed_wrapped_thought(binary: &str, state: &std::path::Path) -> String {
    let created = json_command(binary, state, &[]);
    let session = created["data"]["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    let content = "a".repeat(100);
    let _added = json_input_command(binary, state, &["thoughts", "add", &session], &content);
    session
}

fn rendered_cursor_after_visual_end(sequence: &str) -> (u16, u16) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let session = seed_wrapped_thought(binary, state.path());
    let transcript = state.path().join("cursor.transcript");
    let script = r#"
        log_user 0
        set timeout 10
        set stty_init "rows 10 columns 24"
        set capture [open $env(PROQI_TEST_TRANSCRIPT) "w"]
        fconfigure $capture -translation binary -encoding binary
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        puts -nonewline $capture "\x1b\[?1049h"
        after 200
        send -- "\r"
        after 100
        send -- "\x1b\[H"
        for {set index 0} {$index < 5} {incr index} {
            send -- "\x1b\[C"
        }
        if {$env(PROQI_TEST_SEQUENCE) ne ""} {
            send -- $env(PROQI_TEST_SEQUENCE)
        }
        after 200
        set timeout 1
        expect {
            -re {.+} {
                puts -nonewline $capture $expect_out(buffer)
                exp_continue
            }
            timeout {}
        }
        close $capture
        set timeout 10
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
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_TRANSCRIPT", &transcript)
        .env("PROQI_TEST_SEQUENCE", sequence)
        .status()
        .expect("run visual-row cursor PTY workflow");
    assert!(
        status.success(),
        "visual-row cursor PTY exited with {status}"
    );

    let bytes = std::fs::read(transcript).expect("read cursor transcript");
    let mut parser = vt100::Parser::new(10, 24, 0);
    parser.process(&bytes);
    parser.screen().cursor_position()
}

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

#[test]
fn macos_primary_right_stays_on_the_current_wrapped_row_end() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let interior = rendered_cursor_after_visual_end("");
    let at_end = rendered_cursor_after_visual_end("\x1b[1;9C");
    let repeated = rendered_cursor_after_visual_end("\x1b[1;9C\x1b[1;9C");

    assert_eq!(
        at_end.0, interior.0,
        "visual-row end changed rows: interior={interior:?}, end={at_end:?}"
    );
    assert!(
        at_end.1 > interior.1,
        "visual-row end did not move right: interior={interior:?}, end={at_end:?}"
    );
    assert_eq!(repeated, at_end, "repeated visual-row end did not converge");
}
