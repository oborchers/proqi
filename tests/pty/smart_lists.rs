//! Smart-list editing, exact indentation, and restart-safe history in a PTY.

use super::support::{consume_first_run, expect_command, json_command};

#[test]
fn tab_and_terminal_backtab_persist_with_restart_safe_history() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    run_tab_session(binary, state.path());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thought = thoughts["data"]["thoughts"][0]["id"]
        .as_str()
        .expect("thought ID");
    assert_content(
        binary,
        state.path(),
        session,
        thought,
        "- parent\n  - child",
    );
    assert_history_cycle(
        binary,
        state.path(),
        session,
        thought,
        "- parent\n- child",
        "- parent\n  - child",
    );

    run_backtab_session(binary, state.path(), session);
    assert_content(binary, state.path(), session, thought, "- parent\n- child");
    assert_history_cycle(
        binary,
        state.path(),
        session,
        thought,
        "- parent\n  - child",
        "- parent\n- child",
    );
}

fn run_tab_session(binary: &str, state: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[200~- parent\n- child\x1b\[201~"
        after 150
        send "\t"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect {
            eof {}
            timeout { exit 93 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    run_pty(script, binary, state, None);
}

fn run_backtab_session(binary: &str, state: &std::path::Path, session: &str) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        expect -exact "\x1b\[?1049h"
        after 300
        send "\r"
        after 100
        send -- "\x1b\[Z"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect {
            eof {}
            timeout { exit 93 }
        }
        catch wait result
        exit [lindex $result 3]
    "#;
    run_pty(script, binary, state, Some(session));
}

fn run_pty(script: &str, binary: &str, state: &std::path::Path, session: Option<&str>) {
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state);
    if let Some(session) = session {
        command.env("PROQI_TEST_SESSION", session);
    }
    let status = command.status().expect("run smart-list PTY workflow");
    assert!(status.success(), "smart-list PTY exited with {status}");
}

fn assert_history_cycle(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    thought: &str,
    undone: &str,
    redone: &str,
) {
    let _undo = json_command(
        binary,
        state,
        &["thoughts", "undo", session, "--thought", thought],
    );
    assert_content(binary, state, session, thought, undone);
    let _redo = json_command(
        binary,
        state,
        &["thoughts", "redo", session, "--thought", thought],
    );
    assert_content(binary, state, session, thought, redone);
}

fn assert_content(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    thought: &str,
    expected: &str,
) {
    let inspected = json_command(binary, state, &["thoughts", "inspect", session, thought]);
    assert_eq!(inspected["data"]["thought"]["content"], expected);
}
