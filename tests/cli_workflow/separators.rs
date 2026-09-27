//! Real-binary mixed Board-item mutation, replay, and history contracts.

use rusqlite::Connection;
use serde_json::Value;

use super::{add_with_idempotency, create_session, operation_id, run, success};

#[test]
fn separator_commands_preserve_mixed_order_replay_and_board_history_across_processes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let first = add_with_idempotency(root, &session, "first");
    let second = add_with_idempotency(root, &session, "second");
    success(
        root,
        &["thoughts", "rename", &session, &first, "Named source"],
        None,
    );

    let separator = insert_and_assert_separator(root, &session, &first, &second);
    exercise_mixed_item_history(root, &session, &first, &separator);
}

fn insert_and_assert_separator(
    root: &std::path::Path,
    session: &str,
    first: &str,
    second: &str,
) -> String {
    let insert_operation = operation_id();
    let insert_arguments = [
        "items",
        "insert-separator",
        session,
        "--position",
        "1",
        "--operation-id",
        &insert_operation,
    ];
    let inserted = success(root, &insert_arguments, None);
    let separator = inserted["item_ids"][0]["id"]
        .as_str()
        .expect("separator ID")
        .to_owned();
    assert!(separator.starts_with("sep_"));
    assert_eq!(inserted["receipt"]["idempotent_replay"], false);
    let replay = success(root, &insert_arguments, None);
    assert_eq!(replay["item_ids"][0]["id"], separator);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);

    let conflict = run(
        root,
        &[
            "items",
            "insert-separator",
            session,
            "--position",
            "0",
            "--operation-id",
            &insert_operation,
        ],
        None,
    );
    assert!(!conflict.status.success());
    let conflict: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");

    let listed = success(root, &["thoughts", "list", session], None);
    assert_eq!(listed["items"].as_array().expect("items").len(), 3);
    assert_eq!(listed["items"][0]["id"], first);
    assert_eq!(listed["items"][1]["kind"], "separator");
    assert_eq!(listed["items"][1]["id"], separator);
    assert!(listed["items"][1].get("content").is_none());
    assert!(listed["items"][1].get("annotations").is_none());
    assert_eq!(listed["items"][2]["id"], second);
    separator
}

fn exercise_mixed_item_history(
    root: &std::path::Path,
    session: &str,
    first: &str,
    separator: &str,
) {
    success(
        root,
        &[
            "items",
            "move",
            session,
            separator,
            "0",
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    let moved = success(root, &["thoughts", "list", session], None);
    assert_eq!(moved["items"][0]["id"], separator);

    let duplicate_operation = operation_id();
    let duplicate_arguments = [
        "items",
        "duplicate",
        session,
        separator,
        first,
        "--operation-id",
        &duplicate_operation,
    ];
    let duplicated = success(root, &duplicate_arguments, None);
    let duplicate_ids = duplicated["item_ids"].as_array().expect("duplicate IDs");
    assert_eq!(duplicate_ids.len(), 2);
    assert_eq!(duplicate_ids[0]["kind"], "separator");
    assert_eq!(duplicate_ids[1]["kind"], "thought");
    assert_eq!(duplicated["receipt"]["idempotent_replay"], false);
    let duplicate_replay = success(root, &duplicate_arguments, None);
    assert_eq!(duplicate_replay["item_ids"], duplicated["item_ids"]);
    assert_eq!(duplicate_replay["receipt"]["idempotent_replay"], true);
    let duplicate = duplicate_ids[1]["id"].as_str().expect("duplicate thought");
    let inspected = success(root, &["thoughts", "inspect", session, duplicate], None);
    assert_eq!(inspected["thought"]["content"], "first");
    assert_eq!(inspected["thought"]["name"], "Named source");

    exercise_mixed_delete_history(root, session, separator, duplicate);
}

fn exercise_mixed_delete_history(
    root: &std::path::Path,
    session: &str,
    separator: &str,
    duplicate: &str,
) {
    let delete_operation = operation_id();
    let delete_arguments = [
        "items",
        "delete",
        session,
        separator,
        duplicate,
        "--operation-id",
        &delete_operation,
    ];
    let deletion = success(root, &delete_arguments, None);
    assert_eq!(deletion["item_ids"][0]["id"], separator);
    assert_eq!(deletion["item_ids"][1]["id"], duplicate);
    assert_eq!(deletion["receipt"]["idempotent_replay"], false);
    let deletion_replay = success(root, &delete_arguments, None);
    assert_eq!(deletion_replay["item_ids"], deletion["item_ids"]);
    assert_eq!(deletion_replay["receipt"]["idempotent_replay"], true);
    let deleted = success(root, &["thoughts", "list", session], None);
    assert!(
        !deleted["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == separator)
    );
    assert!(
        !deleted["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == duplicate)
    );

    success(root, &["thoughts", "undo", session], None);
    let restored = success(root, &["thoughts", "list", session], None);
    assert!(
        restored["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == separator)
    );
    assert!(
        restored["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == duplicate)
    );
    success(root, &["thoughts", "redo", session], None);
    let redone = success(root, &["thoughts", "list", session], None);
    assert!(
        !redone["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == separator)
    );
}

#[test]
fn mixed_item_commands_reject_wrong_typed_stale_and_repeated_inputs() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let thought = add_with_idempotency(root, &session, "body");
    let second = add_with_idempotency(root, &session, "second");

    assert_out_of_bounds_insert_is_atomic(root, &session);

    let wrong_type = run(
        root,
        &["items", "move", &session, &operation_id(), "0"],
        None,
    );
    assert!(!wrong_type.status.success());
    let wrong_type: Value = serde_json::from_slice(&wrong_type.stdout).expect("wrong type JSON");
    assert_eq!(wrong_type["error"]["code"], "invalid_identifier");

    let repeated = run(
        root,
        &[
            "items",
            "duplicate",
            &session,
            &thought,
            &thought,
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    assert!(!repeated.status.success());

    let reversed = run(
        root,
        &[
            "items",
            "delete",
            &session,
            &second,
            &thought,
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    assert!(!reversed.status.success());
    let reversed: Value = serde_json::from_slice(&reversed.stdout).expect("reversed JSON");
    assert_eq!(reversed["error"]["code"], "invalid_state");

    let stale_separator = format!("sep_{}", &operation_id()[3..]);
    let stale = run(
        root,
        &[
            "items",
            "delete",
            &session,
            &stale_separator,
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    assert!(!stale.status.success());
    let stale: Value = serde_json::from_slice(&stale.stdout).expect("stale JSON");
    assert_eq!(stale["ok"], false);
}

fn assert_out_of_bounds_insert_is_atomic(root: &std::path::Path, session: &str) {
    let output = run(
        root,
        &[
            "items",
            "insert-separator",
            session,
            "--position",
            "99",
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    assert!(!output.status.success());
    let output: Value = serde_json::from_slice(&output.stdout).expect("position JSON");
    assert_eq!(output["error"]["code"], "invalid_state");
    let listed = success(root, &["thoughts", "list", session], None);
    assert_eq!(
        listed["items"].as_array().expect("unchanged items").len(),
        2
    );
}

#[test]
fn one_thought_delete_replays_across_both_public_spellings() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let thought = add_with_idempotency(root, &session, "body");
    let operation = operation_id();
    let deleted = success(
        root,
        &[
            "items",
            "delete",
            &session,
            &thought,
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(deleted["receipt"]["idempotent_replay"], false);
    let replay = success(
        root,
        &[
            "thoughts",
            "delete",
            &session,
            &thought,
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(replay["thought_id"], thought);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);

    let second = add_with_idempotency(root, &session, "second");
    let reverse_operation = operation_id();
    success(
        root,
        &[
            "thoughts",
            "delete",
            &session,
            &second,
            "--operation-id",
            &reverse_operation,
        ],
        None,
    );
    let reverse_replay = success(
        root,
        &[
            "items",
            "delete",
            &session,
            &second,
            "--operation-id",
            &reverse_operation,
        ],
        None,
    );
    assert_eq!(reverse_replay["item_ids"][0]["id"], second);
    assert_eq!(reverse_replay["receipt"]["idempotent_replay"], true);
}

#[test]
fn one_thought_move_replays_across_both_public_spellings() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let first = add_with_idempotency(root, &session, "first");
    let second = add_with_idempotency(root, &session, "second");
    let separator = success(
        root,
        &["items", "insert-separator", &session, "--position", "1"],
        None,
    )["item_ids"][0]["id"]
        .as_str()
        .expect("separator ID")
        .to_owned();
    let operation = operation_id();
    let moved = success(
        root,
        &[
            "items",
            "move",
            &session,
            &second,
            "0",
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(moved["receipt"]["idempotent_replay"], false);
    let replay = success(
        root,
        &[
            "thoughts",
            "move",
            &session,
            &second,
            "0",
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(replay["thought_id"], second);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    let listed = success(root, &["thoughts", "list", &session], None);
    assert_eq!(listed["items"][0]["id"], second);
    assert_eq!(listed["items"][1]["id"], first);
    assert_eq!(listed["items"][2]["id"], separator);
}

#[test]
fn separator_operation_identity_is_bound_to_the_exact_session() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let first = create_session(root);
    let second = create_session(root);
    let operation = operation_id();
    success(
        root,
        &[
            "items",
            "insert-separator",
            &first,
            "--operation-id",
            &operation,
        ],
        None,
    );
    let conflict = run(
        root,
        &[
            "items",
            "insert-separator",
            &second,
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert!(!conflict.status.success());
    let conflict: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    let second_items = success(root, &["thoughts", "list", &second], None);
    assert!(second_items["items"].as_array().expect("items").is_empty());
}

#[test]
fn legacy_null_fingerprint_keeps_structural_replay_and_conflict_rules() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let operation = operation_id();
    let arguments = [
        "items",
        "insert-separator",
        &session,
        "--position",
        "0",
        "--operation-id",
        &operation,
    ];
    let inserted = success(root, &arguments, None);
    let separator = inserted["item_ids"][0]["id"]
        .as_str()
        .expect("separator ID")
        .to_owned();

    Connection::open(root.join("data/proqi.sqlite3"))
        .expect("open fixture database")
        .execute(
            "UPDATE commit_receipts SET semantic_fingerprint = NULL
             WHERE semantic_fingerprint IS NOT NULL",
            [],
        )
        .expect("simulate migrated legacy receipt");

    let replay = success(root, &arguments, None);
    assert_eq!(replay["item_ids"][0]["id"], separator);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);

    let conflict = run(
        root,
        &[
            "items",
            "insert-separator",
            &session,
            "--position",
            "1",
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert!(!conflict.status.success());
    let conflict: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    let listed = success(root, &["thoughts", "list", &session], None);
    assert_eq!(listed["items"].as_array().expect("items").len(), 1);
    assert_eq!(listed["items"][0]["id"], separator);
}
