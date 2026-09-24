//! Real-binary retry safety for session rename, trash, restore, history, and prune.

use std::path::Path;

use serde_json::Value;

use super::{create_session, operation_id, run, success};

fn error(root: &Path, arguments: &[&str]) -> (Option<i32>, Value) {
    let output = run(root, arguments, None);
    let value: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(value["ok"], false, "{arguments:?}");
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

fn assert_receipt(data: &Value, operation: &str, replay: bool) {
    assert_eq!(data["receipt"]["operation_id"], operation);
    assert_eq!(data["receipt"]["idempotent_replay"], replay);
}

#[test]
fn rename_replays_after_later_changes_and_rejects_divergent_reuse() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let operation = operation_id();
    let rename = [
        "sessions",
        "rename",
        &session,
        "first",
        "--operation-id",
        &operation,
    ];

    let applied = success(root, &rename, None);
    assert_eq!(applied["changed"], true);
    assert_receipt(&applied, &operation, false);
    success(root, &["sessions", "rename", &session, "later"], None);
    let replay = success(root, &rename, None);
    assert_receipt(&replay, &operation, true);
    assert_eq!(replay["changed"], false);
    assert_eq!(
        name_of(root, &session),
        "later",
        "a replay must not reapply"
    );

    let (exit, divergent) = error(
        root,
        &[
            "sessions",
            "rename",
            &session,
            "other",
            "--operation-id",
            &operation,
        ],
    );
    assert_eq!(exit, Some(7));
    assert_eq!(divergent["code"], "idempotency_conflict");
    let (_, clear) = error(
        root,
        &[
            "sessions",
            "rename",
            &session,
            "--clear",
            "--operation-id",
            &operation,
        ],
    );
    assert_eq!(clear["code"], "idempotency_conflict");

    let unchanged = operation_id();
    let same = success(
        root,
        &[
            "sessions",
            "rename",
            &session,
            "later",
            "--operation-id",
            &unchanged,
        ],
        None,
    );
    assert_eq!(same["changed"], false);
    assert_receipt(&same, &unchanged, false);
    let (_, reused) = error(
        root,
        &[
            "sessions",
            "rename",
            &session,
            "else",
            "--operation-id",
            &unchanged,
        ],
    );
    assert_eq!(reused["code"], "idempotency_conflict");
}

#[test]
fn repeated_trash_is_an_unchanged_success_and_restore_prune_replay() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let trash = operation_id();
    let trashed = success(
        root,
        &["sessions", "trash", &session, "--operation-id", &trash],
        None,
    );
    assert_eq!(trashed["status"], "trashed");
    assert_eq!(trashed["changed"], true);
    for _ in 0..3 {
        let again = success(root, &["sessions", "trash", &session], None);
        assert_eq!(again["status"], "trashed");
        assert_eq!(again["changed"], false);
        assert_eq!(again["receipt"]["idempotent_replay"], false);
    }
    let replay = success(
        root,
        &["sessions", "trash", &session, "--operation-id", &trash],
        None,
    );
    assert_receipt(&replay, &trash, true);

    let restore = operation_id();
    let restored = success(
        root,
        &["sessions", "restore", &session, "--operation-id", &restore],
        None,
    );
    assert_eq!(restored["changed"], true);
    let restore_replay = success(
        root,
        &["sessions", "restore", &session, "--operation-id", &restore],
        None,
    );
    assert_receipt(&restore_replay, &restore, true);
    let (exit, live) = error(root, &["sessions", "restore", &session]);
    assert_eq!(exit, Some(7));
    assert_eq!(live["code"], "session_not_trashed");
    let (_, trash_replay_after_restore) = error(
        root,
        &["sessions", "restore", &session, "--operation-id", &trash],
    );
    assert_eq!(trash_replay_after_restore["code"], "idempotency_conflict");

    let (_, prune_live) = error(root, &["sessions", "prune", &session, "--yes"]);
    assert_eq!(prune_live["code"], "session_not_trashed");
    success(root, &["sessions", "trash", &session], None);
    let prune = operation_id();
    let pruned = success(
        root,
        &[
            "sessions",
            "prune",
            &session,
            "--yes",
            "--operation-id",
            &prune,
        ],
        None,
    );
    assert_eq!(pruned["status"], "pruned");
    let prune_replay = success(
        root,
        &[
            "sessions",
            "prune",
            &session,
            "--yes",
            "--operation-id",
            &prune,
        ],
        None,
    );
    assert_receipt(&prune_replay, &prune, true);
    let (exit, absent) = error(root, &["sessions", "prune", &session, "--yes"]);
    assert_eq!(exit, Some(3));
    assert_eq!(absent["code"], "not_found");
}

#[test]
fn browser_history_moves_replay_by_direction() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    success(root, &["sessions", "rename", &session, "named"], None);
    let undo = operation_id();
    let undone = success(root, &["sessions", "undo", "--operation-id", &undo], None);
    assert_eq!(undone["operation"], "session rename");
    assert_receipt(&undone, &undo, false);
    assert!(name_of(root, &session).is_null());

    let replay = success(root, &["sessions", "undo", "--operation-id", &undo], None);
    assert_receipt(&replay, &undo, true);
    assert_eq!(replay["cursor"], undone["cursor"]);
    assert_eq!(replay["operation"], "session rename");
    assert!(
        name_of(root, &session).is_null(),
        "a replay must not undo again"
    );

    let (_, redo_with_undo_identity) = error(root, &["sessions", "redo", "--operation-id", &undo]);
    assert_eq!(redo_with_undo_identity["code"], "idempotency_conflict");
    let redo = operation_id();
    success(root, &["sessions", "redo", "--operation-id", &redo], None);
    assert_eq!(name_of(root, &session), "named");
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
fn retries_may_address_a_session_by_the_name_their_request_changed() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    success(root, &["sessions", "rename", &session, "old"], None);
    let rename = operation_id();
    let arguments = [
        "sessions",
        "rename",
        "old",
        "new",
        "--operation-id",
        &rename,
    ];
    success(root, &arguments, None);
    let replay = success(root, &arguments, None);
    assert_receipt(&replay, &rename, true);
    assert_eq!(replay["session_id"], session);
    assert_eq!(name_of(root, &session), "new");

    let other = create_session(root);
    success(root, &["sessions", "rename", &other, "old"], None);
    let reused_name = success(root, &arguments, None);
    assert_receipt(&reused_name, &rename, true);
    assert_eq!(
        reused_name["session_id"], session,
        "the recorded session wins"
    );
    assert_eq!(
        name_of(root, &other),
        "old",
        "the session now named old is untouched"
    );
    for divergent in [
        vec![
            "sessions",
            "rename",
            "old",
            "else",
            "--operation-id",
            &rename,
        ],
        vec!["sessions", "trash", "old", "--operation-id", &rename],
    ] {
        let (exit, conflict) = error(root, &divergent);
        assert_eq!(exit, Some(7), "{divergent:?}");
        assert_eq!(conflict["code"], "idempotency_conflict", "{divergent:?}");
    }
    assert_eq!(
        name_of(root, &other),
        "old",
        "divergent reuse leaves it untouched"
    );
    assert_eq!(name_of(root, &session), "new");
    success(root, &["sessions", "rename", &other, "--clear"], None);

    let (exit, unknown) = error(root, &["sessions", "rename", "old", "new"]);
    assert_eq!(
        exit,
        Some(3),
        "without an identity the stale name stays unknown"
    );
    assert_eq!(unknown["code"], "session_not_found");

    success(root, &["sessions", "trash", "new"], None);
    let prune = operation_id();
    let prune_arguments = [
        "sessions",
        "prune",
        "new",
        "--yes",
        "--operation-id",
        &prune,
    ];
    success(root, &prune_arguments, None);
    let prune_replay = success(root, &prune_arguments, None);
    assert_receipt(&prune_replay, &prune, true);
    assert_eq!(prune_replay["session_id"], session);
}

#[test]
fn a_history_retry_after_its_target_is_pruned_still_replays() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let pruned = create_session(root);
    let other = create_session(root);
    success(root, &["sessions", "trash", &pruned], None);
    let undo = operation_id();
    let undone = success(root, &["sessions", "undo", "--operation-id", &undo], None);
    success(root, &["sessions", "trash", &pruned], None);
    success(root, &["sessions", "prune", &pruned, "--yes"], None);
    success(root, &["sessions", "rename", &other, "later"], None);

    let replay = success(root, &["sessions", "undo", "--operation-id", &undo], None);
    assert_receipt(&replay, &undo, true);
    assert_eq!(replay["cursor"], undone["cursor"]);
    assert_eq!(
        name_of(root, &other),
        "later",
        "the retry must not undo another entry"
    );
}
