//! Active-owner failure, retry, and replay for semantic mixed-item writes.

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator};
use rusqlite::Connection;
use serde_json::Value;

use super::support::{
    expect_command, json_command, raw_input_command, wait_for_control_owner, wait_for_path,
};

#[test]
fn failed_active_separator_save_has_no_phantom_receipt_and_exact_retry_replays() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("failure-ready");
    let failed = state.path().join("persistence-failed");
    let retry = state.path().join("retry");
    let done = state.path().join("failure-done");
    let mut owner = spawn_failing_owner(
        binary,
        state.path(),
        session,
        &ready,
        &failed,
        &retry,
        &done,
    );
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);

    let operation = operation_id();
    let arguments = [
        "items",
        "insert-separator",
        session,
        "--operation-id",
        &operation,
    ];
    let first = raw_input_command(binary, state.path(), &arguments, "");
    assert!(!first.status.success());
    let first: Value = serde_json::from_slice(&first.stdout).expect("failure JSON");
    assert_eq!(first["error"]["code"], "storage_busy");
    wait_for_path(&failed);
    assert_eq!(durable_separator_count(state.path()), 0);

    std::fs::write(&retry, b"retry").expect("release exact retry");
    wait_for_durable_separator(state.path());
    let replay = json_command(binary, state.path(), &arguments);
    assert_eq!(replay["data"]["receipt"]["sequence"], 1);
    assert_eq!(replay["data"]["receipt"]["idempotent_replay"], true);
    let separator = replay["data"]["item_ids"][0]["id"]
        .as_str()
        .expect("separator ID");
    let listed = json_command(binary, state.path(), &["thoughts", "list", session]);
    let matches = listed["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|item| item["id"] == separator)
        .count();
    assert_eq!(matches, 1);
    assert_eq!(durable_separator_count(state.path()), 1);
    assert_eq!(
        durable_fingerprint_length(state.path(), &operation),
        Some(32)
    );

    std::fs::write(&done, b"done").expect("release owner");
    let status = owner.wait().expect("wait for owner");
    assert!(status.success(), "failing owner PTY exited with {status}");
}

fn spawn_failing_owner(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    ready: &std::path::Path,
    failed: &std::path::Path,
    retry: &std::path::Path,
    done: &std::path::Path,
) -> std::process::Child {
    let script = r#"
        log_user 0
        set timeout 15
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        set deadline [expr {[clock milliseconds] + 30000}]
        while {![file exists $env(PROQI_TEST_RETRY)]} {
            if {[clock milliseconds] >= $deadline} { exit 91 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 92 }
            }
            after 20
        }
        send -- "r"
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 93 }
            expect -timeout 0 {
                -re ".+" { exp_continue }
                timeout {}
                eof { exit 94 }
            }
            after 20
        }
        send "\x1b"
        after 100
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_RETRY", retry)
        .env("PROQI_TEST_DONE", done)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_PERSISTENCE_DELAY_MS", "300")
        .env("PROQI_TEST_PERSISTENCE_FAIL_ONCE", "1")
        .env("PROQI_TEST_PERSISTENCE_FAILED", failed)
        .spawn()
        .expect("spawn failing owner")
}

fn wait_for_durable_separator(state: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while durable_separator_count(state) != 1 {
        assert!(
            std::time::Instant::now() < deadline,
            "separator retry did not become durable"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn operation_id() -> String {
    SystemIdGenerator.operation_id().to_string()
}

fn durable_separator_count(state: &std::path::Path) -> i64 {
    Connection::open(state.join("data/proqi.sqlite3"))
        .expect("open isolated database")
        .query_row("SELECT count(*) FROM separators", [], |row| row.get(0))
        .expect("count durable separators")
}

fn durable_fingerprint_length(state: &std::path::Path, operation_id: &str) -> Option<i64> {
    let operation = operation_id
        .parse::<proqi::domain::OperationId>()
        .expect("operation ID");
    Connection::open(state.join("data/proqi.sqlite3"))
        .expect("open isolated database")
        .query_row(
            "SELECT length(semantic_fingerprint) FROM commit_receipts
             WHERE entity_kind = 'operation' AND external_id = ?1",
            [operation.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("semantic fingerprint length")
}
