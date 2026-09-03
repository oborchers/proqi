//! Exact editor durability, rehydration, and persistent undo through a PTY.

use super::support::{consume_first_run, expect_command, json_command};

const ORIGINAL: &str = "This is a test thought.";
const DELETED: &str = "This is a test";

#[test]
fn physical_macos_redo_encoding_restores_the_durable_deletion() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    create_original_thought(binary, state.path());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thought = thoughts["data"]["thoughts"][0]["id"]
        .as_str()
        .expect("thought ID");
    assert_eq!(thoughts["data"]["thoughts"][0]["content"], ORIGINAL);

    apply_edit_input(
        binary,
        state.path(),
        session,
        "\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f",
    );
    assert_persisted_content(binary, state.path(), session, thought, DELETED);

    apply_edit_input(binary, state.path(), session, "\x1b[122;9u");
    assert_persisted_content(binary, state.path(), session, thought, ORIGINAL);

    apply_edit_input(binary, state.path(), session, "\x1b[90;10u");
    assert_persisted_content(binary, state.path(), session, thought, DELETED);
}

fn create_original_thought(binary: &str, state: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~This is a test thought.\x1b\[201~"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run_editor_workflow(script, binary, state, None, None);
}

fn apply_edit_input(binary: &str, state: &std::path::Path, session: &str, input: &str) {
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        set input $env(PROQI_TEST_INPUT)
        spawn $binary --state-dir $state -r $session
        expect -exact "\x1b\[?1049h"
        send "\r"
        after 100
        send -- $input
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    run_editor_workflow(script, binary, state, Some(session), Some(input));
}

fn assert_persisted_content(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    thought: &str,
    expected: &str,
) {
    let inspected = json_command(binary, state, &["thoughts", "inspect", session, thought]);
    assert_eq!(inspected["data"]["thought"]["content"], expected);
}

fn run_editor_workflow(
    script: &str,
    binary: &str,
    state: &std::path::Path,
    session: Option<&str>,
    input: Option<&str>,
) {
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state);
    if let Some(session) = session {
        command.env("PROQI_TEST_SESSION", session);
    }
    if let Some(input) = input {
        command.env("PROQI_TEST_INPUT", input);
    }
    let status = command.status().expect("run editor redo PTY workflow");
    assert!(status.success(), "editor redo PTY exited with {status}");
}

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
