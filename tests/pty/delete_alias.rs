//! Real PTY coverage for Board Delete aliases and durable undo.

use super::support::{expect_command, json_command};

fn run_delete(sequence: &str) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let script = format!(
        r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[200~alpha\x1b\[201~"
        after 150
        send "\x1b"
        after 100
        send -- "{sequence}"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#
    );
    let status = expect_command()
        .args(["-c", &script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY delete workflow");
    assert!(status.success(), "delete PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let deleted = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        deleted["data"]["thoughts"].as_array().map(Vec::len),
        Some(0)
    );

    let undo = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        expect -exact "\x1b\[?1049h"
        after 300
        send "\x1b"
        after 100
        send "u"
        after 500
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", undo])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run PTY undo workflow");
    assert!(status.success(), "undo PTY exited with {status}");
    let restored = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(restored["data"]["thoughts"][0]["content"], "alpha");
}

#[test]
fn d_and_physical_delete_each_commit_one_durable_undoable_deletion() {
    run_delete("d");
    run_delete(r"\x1b\[3~");
}

#[test]
fn physical_delete_removes_one_multi_selection_and_undo_restores_it() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        foreach thought {first second third} {
            send -- "\x1b\[200~$thought\x1b\[201~"
            after 120
            send "\x1b"
            after 80
        }
        send " "
        send "k"
        send " "
        send -- "\x1b\[3~"
        after 500
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
        .expect("run PTY multi-delete workflow");
    assert!(status.success(), "multi-delete PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let remaining = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(remaining["data"]["thoughts"][0]["content"], "first");
    assert_eq!(
        remaining["data"]["thoughts"].as_array().map(Vec::len),
        Some(1)
    );

    let undo = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set session $env(PROQI_TEST_SESSION)
        spawn $binary --state-dir $state -r $session
        expect -exact "\x1b\[?1049h"
        after 300
        send "u"
        after 500
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", undo])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .status()
        .expect("run PTY multi-delete undo");
    assert!(status.success(), "multi-delete undo exited with {status}");
    let restored = json_command(binary, state.path(), &["thoughts", "list", session]);
    let contents = restored["data"]["thoughts"]
        .as_array()
        .expect("thoughts")
        .iter()
        .map(|thought| thought["content"].as_str().expect("content"))
        .collect::<Vec<_>>();
    assert_eq!(contents, ["first", "second", "third"]);
}
