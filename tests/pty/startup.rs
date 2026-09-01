//! Startup input admission and resize ordering after terminal ownership.

use super::support::{consume_first_run, expect_command, json_command};

use rusqlite::Connection;

#[test]
fn empty_startup_prompt_and_focus_collapse_consume_no_durable_sequence() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let startup = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        set stty_init "rows 8 columns 48"
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        expect {
            -exact "Start" {}
            timeout { exit 90 }
            eof { exit 91 }
        }
        expect {
            -exact "typing" {}
            timeout { exit 92 }
            eof { exit 93 }
        }
        send "\x1b"
        after 100
        send -- "n"
        after 100
        send -- "\x1b\[O"
        expect {
            -exact "Start" {}
            timeout { exit 94 }
            eof { exit 95 }
        }
        expect {
            -exact "typing" {}
            timeout { exit 96 }
            eof { exit 97 }
        }
        send "\x1b"
        after 100
        send -- "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", startup])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY empty Compose presentation workflow");
    assert!(status.success());

    let database = state.path().join("data/proqi.sqlite3");
    let connection = Connection::open(database).expect("open durable state");
    let durable = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM thoughts),
                (SELECT count(*) FROM board_operations),
                (SELECT count(*) FROM thought_revisions),
                (SELECT count(*) FROM commit_receipts),
                (SELECT max(last_durable_sequence) FROM sessions)",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .expect("read zero-durability oracle");
    assert_eq!(durable, (0, 0, 0, 0, 0));
}

#[test]
fn startup_typeahead_after_terminal_ownership_is_not_lost() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let startup = r#"
        log_user 0
        set timeout 10
        set binary $env(PROQI_TEST_BINARY)
        set state $env(PROQI_TEST_STATE)
        spawn $binary --state-dir $state
        expect -exact "\x1b\[?1049h"
        send -- "\x1b\[200~startup-typeahead\x1b\[201~"
        after 500
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", startup])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .status()
        .expect("run PTY startup typeahead workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "startup-typeahead"
    );
}

#[test]
fn keyboard_creation_survives_rapid_pty_resize() {
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
        send -- "mouse-created"
        stty rows 4 columns 12
        after 100
        stty rows 30 columns 100
        after 100
        stty rows 6 columns 20
        after 500
        send "\x1b"
        after 100
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
        .expect("run PTY keyboard and resize workflow");
    assert!(status.success());

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(thoughts["data"]["thoughts"][0]["content"], "mouse-created");
}
