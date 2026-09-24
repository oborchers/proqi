//! Real-binary atomic creation of a named thought on an inactive session.

use std::path::Path;

use serde_json::Value;

use super::{create_session, operation_id, run, success};

fn list_items(root: &Path, session: &str) -> Vec<Value> {
    success(root, &["thoughts", "list", session], None)["items"]
        .as_array()
        .expect("items")
        .clone()
}

#[test]
fn named_add_is_one_undoable_operation_with_retry_and_conflict_safety() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    success(root, &["thoughts", "add", &session], Some("existing"));
    let operation = operation_id();
    let arguments = [
        "thoughts",
        "add",
        &session,
        "--name",
        "Review plan",
        "--position",
        "0",
        "--operation-id",
        &operation,
    ];
    let body = "Grüße 👩‍💻\r\nline two\n";
    let added = success(root, &arguments, Some(body));
    let thought = added["thought_id"].as_str().expect("thought ID");
    assert_eq!(added["receipt"]["idempotent_replay"], false);

    let items = list_items(root, &session);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], thought);
    assert_eq!(items[0]["name"], "Review plan");
    assert_eq!(items[0]["content"], body);

    let replay = success(root, &arguments, Some(body));
    assert_eq!(replay["thought_id"], thought);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    assert_eq!(list_items(root, &session).len(), 2, "replay adds nothing");

    let renamed = run(
        root,
        &[
            "thoughts",
            "add",
            &session,
            "--name",
            "Other",
            "--position",
            "0",
            "--operation-id",
            &operation,
        ],
        Some(body),
    );
    let error: Value = serde_json::from_slice(&renamed.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "idempotency_conflict");
    let unnamed = run(
        root,
        &[
            "thoughts",
            "add",
            &session,
            "--position",
            "0",
            "--operation-id",
            &operation,
        ],
        Some(body),
    );
    let error: Value = serde_json::from_slice(&unnamed.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "idempotency_conflict");

    success(root, &["thoughts", "undo", &session], None);
    let after_undo = list_items(root, &session);
    assert_eq!(
        after_undo.len(),
        1,
        "one undo removes content and name together"
    );
    assert_eq!(after_undo[0]["content"], "existing");
    success(root, &["thoughts", "redo", &session], None);
    let after_redo = list_items(root, &session);
    assert_eq!(after_redo[0]["name"], "Review plan");
    assert_eq!(after_redo[0]["content"], body);
}

#[test]
fn invalid_names_fail_before_reading_or_writing_content() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let long = "n".repeat(81);
    for name in ["", "   ", "two\nlines", "tab\tname", long.as_str()] {
        let output = run(
            root,
            &["thoughts", "add", &session, "--name", name],
            Some("must not be stored"),
        );
        assert_eq!(output.status.code(), Some(2), "{name:?}");
        let error: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
        assert_eq!(error["error"]["code"], "invalid_input", "{name:?}");
    }
    let exact = "n".repeat(80);
    let trimmed = success(
        root,
        &[
            "thoughts",
            "add",
            &session,
            "--name",
            &format!("  {exact}  "),
        ],
        Some("kept"),
    );
    let items = list_items(root, &session);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], trimmed["thought_id"]);
    assert_eq!(items[0]["name"], exact);
}
