//! Named creation and session operation identities through one real active TUI owner.

use serde_json::Value;

use super::{operation_id, spawn_owner};
use crate::support::{
    json_command, json_input_command, raw_input_command, wait_for_control_owner, wait_for_path,
};

#[test]
fn active_owner_applies_named_add_and_retry_safe_session_rename() {
    let state = tempfile::tempdir().expect("temporary state");
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = workspace.path().to_str().expect("UTF-8 workspace");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let ensured = json_command(
        binary,
        state.path(),
        &["sessions", "ensure", "--name", "owner-lane", "--cwd", cwd],
    );
    let session = ensured["data"]["session_id"].as_str().expect("session ID");
    let ready = state.path().join("named-owner-ready");
    let done = state.path().join("named-owner-done");
    let mut owner = spawn_owner(binary, state.path(), session, &ready, &done);
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);

    let active = json_command(
        binary,
        state.path(),
        &["sessions", "ensure", "--name", "owner-lane", "--cwd", cwd],
    );
    assert_eq!(active["data"]["session_id"], session);
    assert_eq!(active["data"]["disposition"], "reused");
    assert_eq!(active["data"]["state"], "active");

    let thought = forwarded_named_add(binary, state.path(), session);
    forwarded_session_rename(binary, state.path(), session);

    std::fs::write(&done, b"done").expect("release owner workflow");
    let status = owner.wait().expect("wait for owner workflow");
    assert!(status.success(), "named owner PTY exited with {status}");
    let items = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(items["data"]["items"][0]["id"], thought);
    assert_eq!(items["data"]["items"][0]["name"], "Forwarded name");
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["name"], "Overlapped live");
}

fn forwarded_named_add(binary: &str, state: &std::path::Path, session: &str) -> String {
    let operation = operation_id();
    let arguments = [
        "thoughts",
        "add",
        session,
        "--name",
        "Forwarded name",
        "--operation-id",
        &operation,
    ];
    let added = json_input_command(binary, state, &arguments, "Forwarded Grüße\n");
    let thought = added["data"]["thought_id"]
        .as_str()
        .expect("thought ID")
        .to_owned();
    assert_eq!(added["data"]["receipt"]["idempotent_replay"], false);
    let listed = json_command(binary, state, &["thoughts", "list", session]);
    assert_eq!(listed["data"]["items"][0]["name"], "Forwarded name");
    assert_eq!(listed["data"]["items"][0]["content"], "Forwarded Grüße\n");

    let replay = json_input_command(binary, state, &arguments, "Forwarded Grüße\n");
    assert_eq!(replay["data"]["thought_id"], thought);
    assert_eq!(replay["data"]["receipt"]["idempotent_replay"], true);
    let conflict = raw_input_command(
        binary,
        state,
        &[
            "thoughts",
            "add",
            session,
            "--name",
            "Different",
            "--operation-id",
            &operation,
        ],
        "Forwarded Grüße\n",
    );
    let conflict: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");

    json_command(binary, state, &["thoughts", "undo", session]);
    let undone = json_command(binary, state, &["thoughts", "list", session]);
    assert_eq!(
        undone["data"]["total"], 0,
        "one undo removes the named thought"
    );
    json_command(binary, state, &["thoughts", "redo", session]);
    thought
}

fn forwarded_session_rename(binary: &str, state: &std::path::Path, session: &str) {
    let operation = operation_id();
    let arguments = [
        "sessions",
        "rename",
        session,
        "Renamed live",
        "--operation-id",
        &operation,
    ];
    let renamed = json_command(binary, state, &arguments);
    assert_eq!(renamed["data"]["changed"], true);
    assert_eq!(renamed["data"]["receipt"]["operation_id"], operation);
    let replay = json_command(binary, state, &arguments);
    assert_eq!(replay["data"]["receipt"]["idempotent_replay"], true);
    let conflict = raw_input_command(
        binary,
        state,
        &[
            "sessions",
            "rename",
            session,
            "Other",
            "--operation-id",
            &operation,
        ],
        "",
    );
    let conflict: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    let unchanged_operation = operation_id();
    let unchanged = json_command(
        binary,
        state,
        &[
            "sessions",
            "rename",
            session,
            "Renamed live",
            "--operation-id",
            &unchanged_operation,
        ],
    );
    assert_eq!(unchanged["data"]["changed"], false);
    let reserved = raw_input_command(
        binary,
        state,
        &[
            "sessions",
            "rename",
            session,
            "Something else",
            "--operation-id",
            &unchanged_operation,
        ],
        "",
    );
    let reserved: Value = serde_json::from_slice(&reserved.stdout).expect("reserved JSON");
    assert_eq!(
        reserved["error"]["code"], "idempotency_conflict",
        "a forwarded unchanged rename reserves its identity"
    );
    overlapping_duplicate_renames(binary, state, session);
    let trash = raw_input_command(binary, state, &["sessions", "trash", session], "");
    let trash: Value = serde_json::from_slice(&trash.stdout).expect("trash JSON");
    assert_eq!(trash["error"]["code"], "session_busy");
}

fn overlapping_duplicate_renames(binary: &str, state: &std::path::Path, session: &str) {
    let operation = operation_id();
    let arguments = [
        "sessions",
        "rename",
        session,
        "Overlapped live",
        "--operation-id",
        &operation,
    ];
    let replays = std::thread::scope(|scope| {
        let calls = (0..4)
            .map(|_| scope.spawn(|| json_command(binary, state, &arguments)))
            .collect::<Vec<_>>();
        calls
            .into_iter()
            .map(|call| {
                let renamed = call.join().expect("overlapping rename");
                assert_eq!(renamed["data"]["receipt"]["operation_id"], operation);
                renamed["data"]["receipt"]["idempotent_replay"] == true
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        replays.iter().filter(|replay| !**replay).count(),
        1,
        "exactly one overlapping duplicate applies the rename: {replays:?}"
    );
}
