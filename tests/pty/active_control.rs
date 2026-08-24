//! Cross-process active-owner acceptance through a real pseudo-terminal.

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator};
use serde_json::Value;

use super::{
    expect_command, json_command, json_input_command, raw_input_command, wait_for_control_owner,
    wait_for_path,
};

#[test]
fn active_tui_accepts_durable_idempotent_cli_mutations_before_crash() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let source = json_command(binary, state.path(), &[]);
    let source_id = source["data"]["session_id"].as_str().expect("source ID");
    let source_operation = operation_id();
    let source_thought = json_input_command(
        binary,
        state.path(),
        &[
            "thoughts",
            "add",
            source_id,
            "--operation-id",
            &source_operation,
        ],
        "Transferred while destination is active",
    );
    let source_thought_id = source_thought["data"]["thought_id"]
        .as_str()
        .expect("source thought ID");
    let ready = state.path().join("owner-ready");
    let done = state.path().join("owner-done");
    let mut owner = spawn_owner(binary, state.path(), session, &ready, &done);
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);

    let first_operation = operation_id();
    let first_args = [
        "thoughts",
        "add",
        session,
        "--operation-id",
        &first_operation,
    ];
    let first_body = "Forwarded Grüße 界\nsecond line";
    let first = json_input_command(binary, state.path(), &first_args, first_body);
    let first_id = first["data"]["thought_id"].as_str().expect("first ID");
    assert_equivalent_commit_shape(&source_thought, &first);
    let replay = json_input_command(binary, state.path(), &first_args, first_body);
    let conflict = raw_input_command(binary, state.path(), &first_args, "changed");
    assert_idempotency(&first, &replay, &conflict);
    assert_cross_kind_reuse_is_rejected(binary, state.path(), session, first_id, &first_operation);

    let second_operation = operation_id();
    let second_args = [
        "thoughts",
        "add",
        session,
        "--operation-id",
        &second_operation,
    ];
    let second = json_input_command(binary, state.path(), &second_args, "Keep me");
    let second_id = second["data"]["thought_id"].as_str().expect("second ID");
    mutate_active(binary, state.path(), session, first_id, second_id);
    let transferred = json_command(
        binary,
        state.path(),
        &[
            "thoughts",
            "send",
            source_id,
            source_thought_id,
            session,
            "--operation-id",
            &operation_id(),
        ],
    );
    let transferred_id = transferred["data"]["destination_thought_id"]
        .as_str()
        .expect("transferred thought ID");

    std::fs::write(&done, b"done").expect("release owner workflow");
    let status = owner.wait().expect("wait for active owner workflow");
    assert!(status.success(), "active owner PTY exited with {status}");
    assert_recovered_state(binary, state.path(), session, second_id, transferred_id);
}

fn spawn_owner(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    ready: &std::path::Path,
    done: &std::path::Path,
) -> std::process::Child {
    let script = r#"
        log_user 0
        set timeout 12
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
        expect -exact "\x1b\[?1049h"
        close [open $env(PROQI_TEST_READY) w]
        set deadline [expr {[clock milliseconds] + 12000}]
        while {![file exists $env(PROQI_TEST_DONE)]} {
            if {[clock milliseconds] >= $deadline} { exit 91 }
            after 20
        }
        system /bin/kill -KILL [exp_pid]
        expect eof
        exit 0
    "#;
    expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_READY", ready)
        .env("PROQI_TEST_DONE", done)
        .spawn()
        .expect("spawn active owner workflow")
}

fn assert_idempotency(first: &Value, replay: &Value, conflict: &std::process::Output) {
    assert_eq!(first["data"]["receipt"]["idempotent_replay"], false);
    assert_eq!(replay["data"]["receipt"]["idempotent_replay"], true);
    assert!(!conflict.status.success());
    let error: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(error["error"]["code"], "idempotency_conflict");
}

fn assert_equivalent_commit_shape(inactive: &Value, forwarded: &Value) {
    for value in [inactive, forwarded] {
        assert_eq!(value["data"]["receipt"]["sequence"], 1);
        assert!(
            value["data"]["receipt"]["operation_id"]
                .as_str()
                .is_some_and(|identity| identity.starts_with("op_"))
        );
        assert_eq!(value["data"]["receipt"]["idempotent_replay"], false);
    }
}

fn assert_cross_kind_reuse_is_rejected(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    thought: &str,
    operation: &str,
) {
    let output = raw_input_command(
        binary,
        state,
        &[
            "thoughts",
            "delete",
            session,
            thought,
            "--operation-id",
            operation,
        ],
        "",
    );
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).expect("reuse error JSON");
    assert_eq!(error["error"]["code"], "idempotency_conflict");
}

fn mutate_active(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    first_id: &str,
    second_id: &str,
) {
    let move_operation = operation_id();
    let delete_operation = operation_id();
    let undo_operation = operation_id();
    let redo_operation = operation_id();
    json_command(
        binary,
        state,
        &[
            "thoughts",
            "move",
            session,
            second_id,
            "0",
            "--operation-id",
            &move_operation,
        ],
    );
    json_command(
        binary,
        state,
        &[
            "thoughts",
            "delete",
            session,
            first_id,
            "--operation-id",
            &delete_operation,
        ],
    );
    json_command(
        binary,
        state,
        &[
            "thoughts",
            "undo",
            session,
            "--operation-id",
            &undo_operation,
        ],
    );
    json_command(
        binary,
        state,
        &[
            "thoughts",
            "redo",
            session,
            "--operation-id",
            &redo_operation,
        ],
    );
}

fn assert_recovered_state(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    surviving_id: &str,
    transferred_id: &str,
) {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "recovered");
    let thoughts = json_command(binary, state, &["thoughts", "list", session]);
    let live = thoughts["data"]["thoughts"].as_array().expect("thoughts");
    assert_eq!(live.len(), 2);
    assert_eq!(live[0]["id"], surviving_id);
    assert_eq!(live[0]["content"], "Keep me");
    assert_eq!(live[1]["id"], transferred_id);
    assert_eq!(
        live[1]["content"],
        "Transferred while destination is active"
    );
}

fn operation_id() -> String {
    SystemIdGenerator.operation_id().to_string()
}
