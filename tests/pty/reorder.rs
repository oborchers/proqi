//! Single-thought reordering through normalized modified terminal input.

use super::support::{consume_first_run, expect_command, json_command};

fn option_shift_reorder(sequence: &str) -> Vec<String> {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        foreach thought {first second third} {
            send -- "\x1b\[200~$thought\x1b\[201~"
            after 100
            send "\x1b"
            after 60
        }
        send -- $env(PROQI_TEST_SEQUENCE)
        after 500
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", interact])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SEQUENCE", sequence)
        .status()
        .expect("run PTY Option+Shift reorder workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    json_command(binary, state.path(), &["thoughts", "list", session])["data"]["items"]
        .as_array()
        .expect("thought list")
        .iter()
        .map(|thought| {
            thought["content"]
                .as_str()
                .expect("thought content")
                .to_owned()
        })
        .collect()
}

#[test]
fn option_shift_arrow_bytes_reorder_both_directions_in_a_real_pty() {
    assert_eq!(
        option_shift_reorder("\x1b[1;4A"),
        ["first", "third", "second"]
    );
    assert_eq!(
        option_shift_reorder("\x1b[1;4B"),
        ["third", "first", "second"]
    );
}

#[test]
fn primary_shift_arrow_reorders_one_thought_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[200~first\x1b\[201~"
        after 150
        send "\x1b"
        after 100
        send -- "\x1b\[200~second\x1b\[201~"
        after 150
        send "\x1b"
        after 100
        send -- "\x1b\[200~third\x1b\[201~"
        after 300
        send "\x1b"
        after 100
        send -- "\x1b\[1;10A"
        after 1000
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", interact])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY primary-shift reorder workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let contents = thoughts["data"]["items"]
        .as_array()
        .expect("thought list")
        .iter()
        .map(|thought| thought["content"].as_str().expect("thought content"))
        .collect::<Vec<_>>();
    assert_eq!(contents, ["first", "third", "second"]);
}

#[test]
fn lowercase_primary_shift_k_report_reorders_one_thought_in_a_real_pty() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let interact = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        send -- "\x1b\[200~first\x1b\[201~"
        after 150
        send "\x1b"
        after 100
        send -- "\x1b\[200~second\x1b\[201~"
        after 150
        send "\x1b"
        after 100
        send -- "\x1b\[200~third\x1b\[201~"
        after 300
        send "\x1b"
        after 100
        send -- "\x1b\[107;10u"
        after 1000
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", interact])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY lowercase primary-shift reorder workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let contents = thoughts["data"]["items"]
        .as_array()
        .expect("thought list")
        .iter()
        .map(|thought| thought["content"].as_str().expect("thought content"))
        .collect::<Vec<_>>();
    assert_eq!(contents, ["first", "third", "second"]);
}
