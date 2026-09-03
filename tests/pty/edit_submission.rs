//! Edit and Board submission chord routing through the running terminal product.

use super::support::{consume_first_run, expect_command, json_command};

use rusqlite::Connection;
use std::fmt::Write as _;

#[test]
fn primary_enter_variants_reach_edit_submission_and_retain_an_unroutable_draft() {
    for (sequence, draft) in [
        ("\u{1b}[13;9u", "submit-remove draft"),
        ("\u{1b}[13;10u", "submit-keep draft"),
    ] {
        assert_unavailable_submission_retains_draft(sequence, draft);
    }
}

#[test]
fn primary_enter_variants_reach_board_submission_and_retain_an_unroutable_thought() {
    for (sequence, draft) in [
        ("\u{1b}[13;9u", "board remove Grüße 第二行"),
        ("\u{1b}[13;10u", "board keep 第二行 e\u{301}"),
    ] {
        assert_unavailable_board_submission_retains_thought(sequence, draft);
    }
}

fn assert_unavailable_board_submission_retains_thought(sequence: &str, draft: &str) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let draft_hex = draft
        .as_bytes()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(&mut hex, "{byte:02x}").expect("write byte to in-memory hex string");
            hex
        });
    let script = r#"
        log_user 0
        set timeout 10
        encoding system utf-8
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set sequence $env(PROQI_TEST_SEQUENCE)
        set draft [encoding convertfrom utf-8 [binary format H* $env(PROQI_TEST_DRAFT_HEX)]]
        set stty_init "rows 12 columns 120"
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~$draft\x1b\[201~"
        after 300
        send "\x1b"
        after 100
        send -- $sequence
        expect {
            -exact "direct submission unavailable" {}
            timeout { exit 90 }
            eof { exit 91 }
        }
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SEQUENCE", sequence)
        .env("PROQI_TEST_DRAFT_HEX", draft_hex)
        .status()
        .expect("run Board submission chord in PTY");
    assert!(
        status.success(),
        "Board submission PTY exited with {status}"
    );

    assert_one_thought_without_attempt(binary, state.path(), draft);
}

fn assert_unavailable_submission_retains_draft(sequence: &str, draft: &str) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let script = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set sequence $env(PROQI_TEST_SEQUENCE)
        set draft $env(PROQI_TEST_DRAFT)
        set stty_init "rows 12 columns 120"
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~$draft\x1b\[201~"
        after 500
        send -- $sequence
        expect {
            -exact "direct submission unavailable" {}
            timeout { exit 90 }
            eof { exit 91 }
        }
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
        .env("PROQI_TEST_SEQUENCE", sequence)
        .env("PROQI_TEST_DRAFT", draft)
        .status()
        .expect("run edit submission chord in PTY");
    assert!(status.success(), "edit submission PTY exited with {status}");

    assert_one_thought_without_attempt(binary, state.path(), draft);
}

fn assert_one_thought_without_attempt(binary: &str, state: &std::path::Path, draft: &str) {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state, &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0]["content"], draft);

    let database = state.join("data/proqi.sqlite3");
    let connection = Connection::open(database).expect("open durable state");
    let attempts = connection
        .query_row("SELECT count(*) FROM submission_attempts", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("count submission attempts");
    assert_eq!(attempts, 0);
}
