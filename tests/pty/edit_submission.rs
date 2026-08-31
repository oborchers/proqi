//! Edit-mode submission chord routing through the running terminal product.

use super::support::{expect_command, json_command};

use rusqlite::Connection;

#[test]
fn primary_enter_variants_reach_edit_submission_and_retain_an_unroutable_draft() {
    for (sequence, draft) in [
        ("\u{1b}[13;9u", "submit-remove draft"),
        ("\u{1b}[13;10u", "submit-keep draft"),
    ] {
        assert_unavailable_submission_retains_draft(sequence, draft);
    }
}

fn assert_unavailable_submission_retains_draft(sequence: &str, draft: &str) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
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

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thoughts = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(thoughts.len(), 1);
    assert_eq!(thoughts[0]["content"], draft);

    let database = state.path().join("data/proqi.sqlite3");
    let connection = Connection::open(database).expect("open durable state");
    let attempts = connection
        .query_row("SELECT count(*) FROM submission_attempts", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("count submission attempts");
    assert_eq!(attempts, 0);
}
