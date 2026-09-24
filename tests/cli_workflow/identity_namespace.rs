//! One operation-identity namespace shared by session, thought, and item mutations.

use std::path::Path;

use serde_json::Value;

use super::{create_session, operation_id, run, success};

fn error(root: &Path, arguments: &[&str]) -> (Option<i32>, Value) {
    let output = run(root, arguments, None);
    let value: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    (output.status.code(), value["error"].clone())
}

fn name_of(root: &Path, session: &str) -> Value {
    success(root, &["sessions", "list", "--all"], None)["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|entry| entry["id"] == session)
        .map(|entry| entry["name"].clone())
        .expect("listed session")
}

#[test]
fn session_and_thought_mutations_share_one_identity_namespace() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let operation = operation_id();
    success(
        root,
        &["thoughts", "add", &session, "--operation-id", &operation],
        Some("body"),
    );
    for arguments in [
        vec![
            "sessions",
            "rename",
            &session,
            "x",
            "--operation-id",
            &operation,
        ],
        vec!["sessions", "trash", &session, "--operation-id", &operation],
        vec!["sessions", "undo", "--operation-id", &operation],
    ] {
        let (exit, conflict) = error(root, &arguments);
        assert_eq!(exit, Some(7), "{arguments:?}");
        assert_eq!(conflict["code"], "idempotency_conflict", "{arguments:?}");
    }
    assert!(name_of(root, &session).is_null());
    let (_, malformed) = error(
        root,
        &["sessions", "trash", &session, "--operation-id", "op_short"],
    );
    assert_eq!(malformed["code"], "invalid_identifier");
}

#[test]
fn a_session_identity_reused_for_a_thought_or_item_mutation_conflicts() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let create = operation_id();
    let session = success(
        root,
        &[
            "sessions",
            "create",
            "--name",
            "host",
            "--operation-id",
            &create,
        ],
        None,
    )["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned();
    let rename = operation_id();
    success(
        root,
        &[
            "sessions",
            "rename",
            &session,
            "renamed",
            "--operation-id",
            &rename,
        ],
        None,
    );
    for (arguments, input) in [
        (
            vec!["thoughts", "add", &session, "--operation-id", &create],
            Some("body"),
        ),
        (
            vec![
                "items",
                "insert-separator",
                &session,
                "--operation-id",
                &rename,
            ],
            None,
        ),
        (
            vec!["thoughts", "undo", &session, "--operation-id", &rename],
            None,
        ),
    ] {
        let output = run(root, &arguments, input);
        let value: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
        assert_eq!(output.status.code(), Some(7), "{arguments:?}");
        assert_eq!(
            value["error"]["code"], "idempotency_conflict",
            "{arguments:?}"
        );
    }
    let items = success(root, &["thoughts", "list", &session], None);
    assert_eq!(items["total"], 0, "no conflicting mutation wrote anything");
}
