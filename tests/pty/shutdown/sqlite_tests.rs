//! Regression coverage for the shutdown fixture's owned SQL readiness protocol.

use std::{path::Path, time::Duration};

use rusqlite::Connection;

use crate::{support::expect_command, watchdog};

fn run(database: &Path, transaction: &str, finish: &str) -> std::process::ExitStatus {
    let state = tempfile::tempdir().expect("watchdog state");
    let script = format!(
        r#"
        log_user 0
        set timeout 3
        proc register_watchdog_pid {{pid}} {{
            global env
            set owned [open "$env(PROQI_TEST_STATE)/watchdog-pids" a]
            puts $owned $pid
            close $owned
        }}
        {transaction}
        {finish}
        "#
    );
    let mut command = expect_command();
    command
        .args(["-c", &script])
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_DATABASE", database);
    watchdog::status_before(
        &mut command,
        Duration::from_secs(12),
        &state.path().join("watchdog-pids"),
        "SQLite readiness protocol",
    )
}

#[test]
fn owned_prompts_and_readiness_prove_an_exclusive_writer_is_held() {
    let root = tempfile::tempdir().expect("database root");
    let database = root.path().join("transaction.sqlite3");
    let finish = r#"
        # A separate connection must be denied while the acknowledged lock lives.
        set denied [catch {exec /usr/bin/sqlite3 $env(PROQI_TEST_DATABASE) "BEGIN IMMEDIATE;"} message]
        if {!$denied || [string first "locked" $message] < 0} { exit 97 }
        send -- "SELECT\r"
        expect {
            -exact "proqi-sql-cont> " {}
            timeout { exit 98 }
            eof { exit 98 }
        }
        send -- "1;\r"
        await_sqlite_prompt
        send -- "ROLLBACK;\r"
        await_sqlite_prompt
        send -- ".quit\r"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = run(&database, super::sqlite_transaction(), finish);
    assert!(status.success(), "SQL readiness exited {status}");
    let connection = Connection::open(database).expect("database");
    connection
        .execute_batch("BEGIN IMMEDIATE; ROLLBACK;")
        .expect("lock released");
}

#[test]
fn locked_database_is_rejected_instead_of_acknowledged_ready() {
    let root = tempfile::tempdir().expect("database root");
    let database = root.path().join("locked.sqlite3");
    let connection = Connection::open(&database).expect("database");
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .expect("hold writer");
    assert_eq!(
        run(&database, super::sqlite_transaction(), "exit 99").code(),
        Some(95)
    );
    connection
        .execute_batch("ROLLBACK;")
        .expect("release writer");
}

#[test]
fn sql_syntax_error_is_rejected_before_readiness() {
    let root = tempfile::tempdir().expect("database root");
    let broken = super::sqlite_transaction().replace("BEGIN IMMEDIATE;", "BEGIN INVALID;");
    assert_eq!(
        run(&root.path().join("invalid.sqlite3"), &broken, "exit 99").code(),
        Some(95)
    );
}
