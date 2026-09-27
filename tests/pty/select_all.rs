//! Whole-board selection, range selection, and duplication through real terminal input.

#[cfg(target_os = "macos")]
use super::support::{consume_first_run, expect_command, json_command};

#[cfg(target_os = "macos")]
fn page_then_delete(sequence: &str) -> Vec<String> {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let script = format!(
        r#"
        log_user 0
        set timeout 12
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        after 300
        foreach thought {{one two three four five six seven eight}} {{
            send -- "\x1b\[200~$thought\x1b\[201~"
            after 100
            send "\x1b"
            after 60
        }}
        send -- "{sequence}"
        after 120
        send "d"
        after 500
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
        .expect("run PTY Board paging workflow");
    assert!(status.success(), "Board paging PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    json_command(binary, state.path(), &["thoughts", "list", session])["data"]["items"]
        .as_array()
        .expect("thoughts")
        .iter()
        .map(|thought| thought["content"].as_str().expect("content").to_owned())
        .collect()
}

#[cfg(target_os = "macos")]
#[test]
fn page_bytes_move_or_extend_by_five_before_a_durable_delete() {
    assert_eq!(
        page_then_delete(r"\x1b\[5~"),
        ["one", "two", "four", "five", "six", "seven", "eight"]
    );
    assert_eq!(page_then_delete(r"\x1b\[5;2~"), ["one", "two"]);
    assert_eq!(
        page_then_delete(r"\x1b\[5~\x1b\[6~"),
        ["one", "two", "three", "four", "five", "six", "seven"]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn both_shifted_d_terminal_spellings_duplicate_in_a_real_pty() {
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
        send -- "\x1b\[200~original\x1b\[201~"
        send "\x1b"
        after 100
        send "D"
        after 500
        send "\x1b\[100;2u"
        after 500
        send "\x1b"
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
        .expect("run PTY duplicate workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["items"]
            .as_array()
            .expect("thoughts")
            .iter()
            .map(|thought| thought["content"].as_str())
            .collect::<Vec<_>>(),
        [Some("original"), Some("original"), Some("original")]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn board_key_and_forwarded_primary_a_select_every_thought_in_a_real_pty() {
    for selection_input in ["a", "$env(PROQI_TEST_PRIMARY_A)"] {
        let state = tempfile::tempdir().expect("temporary state");
        let binary = env!("CARGO_BIN_EXE_proqi");
        consume_first_run(binary, state.path());
        let interact = format!(
            r#"
                log_user 0
                set timeout 10
                set binary $env(PROQI_TEST_BINARY)
                set state $env(PROQI_TEST_STATE)
                spawn $binary --state-dir $state
                expect -exact "\x1b\[?1049h"
                after 300
                send -- "\x1b\[200~first\x1b\[201~"
                send "\x1b"
                send -- "\x1b\[200~Grüße 👩‍💻\x1b\[201~"
                send "\x1b"
                send -- "\x1b\[200~第三\x1b\[201~"
                send "\x1b"
                after 200
                send -- "{selection_input}"
                send "d"
                after 500
                send "\x1b"
                after 50
                send "q"
                expect eof
                catch wait result
                exit [lindex $result 3]
            "#
        );
        let status = expect_command()
            .args(["-c", &interact])
            .env("PROQI_TEST_BINARY", binary)
            .env("PROQI_TEST_STATE", state.path())
            .status()
            .expect("run PTY select-all workflow");
        assert!(status.success());

        let sessions = json_command(binary, state.path(), &["sessions", "list"]);
        let session = sessions["data"]["sessions"][0]["id"]
            .as_str()
            .expect("session ID");
        let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
        assert_eq!(thoughts["data"]["items"].as_array().map(Vec::len), Some(0));
    }
}

#[cfg(target_os = "macos")]
#[test]
fn shifted_arrow_range_selection_deletes_one_real_pty_block() {
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
        send -- "\x1b\[200~Grüße 👩‍💻\x1b\[201~"
        after 150
        send "\x1b"
        send -- "\x1b\[200~第三\x1b\[201~"
        after 300
        send "\x1b"
        send -- "\x1b\[1;2A\x1b\[1;2A"
        after 100
        send "d"
        after 500
        send "\x1b"
        after 50
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
        .expect("run PTY range-selection workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(thoughts["data"]["items"].as_array().map(Vec::len), Some(0));
}
