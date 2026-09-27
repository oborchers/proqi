//! Semantic API parity through one real active TUI owner.

use super::{assert_thought_content, operation_id, spawn_owner};
use crate::support::{
    json_command, json_input_command, raw_input_command, wait_for_control_owner, wait_for_path,
};

#[test]
fn active_tui_accepts_semantic_board_mutations_before_crash() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let first = add_thought(binary, state.path(), session, "external replacement");
    let second = add_thought(binary, state.path(), session, "  Keep\t me  ");
    let ready = state.path().join("semantic-owner-ready");
    let done = state.path().join("semantic-owner-done");
    let mut owner = spawn_owner(binary, state.path(), session, &ready, &done);
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);

    exercise(binary, state.path(), session, &first, &second);

    std::fs::write(&done, b"done").expect("release semantic owner workflow");
    let status = owner.wait().expect("wait for semantic owner workflow");
    assert!(status.success(), "semantic owner PTY exited with {status}");
    assert_recovered_state(binary, state.path(), session, &first, &second);
}

fn add_thought(binary: &str, state: &std::path::Path, session: &str, content: &str) -> String {
    json_input_command(
        binary,
        state,
        &[
            "thoughts",
            "add",
            session,
            "--operation-id",
            &operation_id(),
        ],
        content,
    )["data"]["thought_id"]
        .as_str()
        .expect("thought ID")
        .to_owned()
}

fn assert_recovered_state(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    first: &str,
    second: &str,
) {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    assert_eq!(sessions["data"]["sessions"][0]["state"], "recovered");
    let thoughts = json_command(binary, state, &["thoughts", "list", session]);
    let live = thoughts["data"]["items"].as_array().expect("thoughts");
    assert_eq!(live.len(), 2);
    assert_eq!(live[0]["id"], first);
    assert_eq!(live[0]["content"], "external replacement");
    assert_eq!(live[1]["id"], second);
    assert_eq!(live[1]["content"], "Keep me");
}

pub(super) fn exercise(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    first: &str,
    second: &str,
) {
    exercise_items(binary, state, session);
    exercise_transformations(binary, state, session, first, second);
}

fn exercise_items(binary: &str, state: &std::path::Path, session: &str) {
    let insert_operation = operation_id();
    let insert_args = [
        "items",
        "insert-separator",
        session,
        "--position",
        "1",
        "--operation-id",
        &insert_operation,
    ];
    let inserted = json_command(binary, state, &insert_args);
    let separator = inserted["data"]["item_ids"][0]["id"]
        .as_str()
        .expect("separator ID");
    let replay = json_command(binary, state, &insert_args);
    assert_eq!(replay["data"]["receipt"]["idempotent_replay"], true);

    let duplicate_operation = operation_id();
    let duplicate_args = [
        "items",
        "duplicate",
        session,
        separator,
        "--operation-id",
        &duplicate_operation,
    ];
    let duplicated = json_command(binary, state, &duplicate_args);
    let duplicate = duplicated["data"]["item_ids"][0]["id"]
        .as_str()
        .expect("duplicate separator ID");
    let duplicate_replay = json_command(binary, state, &duplicate_args);
    assert_eq!(
        duplicate_replay["data"]["item_ids"],
        duplicated["data"]["item_ids"]
    );
    assert_eq!(
        duplicate_replay["data"]["receipt"]["idempotent_replay"],
        true
    );

    move_and_delete_items(binary, state, session, separator, duplicate);
}

fn move_and_delete_items(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    separator: &str,
    duplicate: &str,
) {
    json_command(
        binary,
        state,
        &[
            "items",
            "move",
            session,
            duplicate,
            "0",
            "--operation-id",
            &operation_id(),
        ],
    );
    let invalid = raw_input_command(
        binary,
        state,
        &[
            "items",
            "delete",
            session,
            separator,
            duplicate,
            "--operation-id",
            &operation_id(),
        ],
        "",
    );
    assert!(!invalid.status.success());
    let invalid: serde_json::Value =
        serde_json::from_slice(&invalid.stdout).expect("active invalid-state JSON");
    assert_eq!(invalid["error"]["code"], "invalid_state");
    let delete_operation = operation_id();
    let delete_args = [
        "items",
        "delete",
        session,
        duplicate,
        separator,
        "--operation-id",
        &delete_operation,
    ];
    let deleted = json_command(binary, state, &delete_args);
    let delete_replay = json_command(binary, state, &delete_args);
    assert_eq!(
        delete_replay["data"]["item_ids"],
        deleted["data"]["item_ids"]
    );
    assert_eq!(delete_replay["data"]["receipt"]["idempotent_replay"], true);
}

fn exercise_transformations(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    first: &str,
    second: &str,
) {
    assert_additive_invalid_state_contract(binary, state, session, first);
    let first_digest = thought_digest(binary, state, session, first);
    let split_operation = operation_id();
    let split_args = [
        "thoughts",
        "split",
        session,
        first,
        "8",
        "--expected-sha256",
        &first_digest,
        "--operation-id",
        &split_operation,
    ];
    let split = json_command(binary, state, &split_args);
    assert_eq!(split["data"]["item_ids"][0]["id"], first);
    let split_replay = json_command(binary, state, &split_args);
    assert_eq!(split_replay["data"]["receipt"]["idempotent_replay"], true);
    json_command(binary, state, &["thoughts", "undo", session]);

    let extract_operation = operation_id();
    let extract_args = [
        "thoughts",
        "extract",
        session,
        first,
        "0",
        "8",
        "--expected-sha256",
        &first_digest,
        "--operation-id",
        &extract_operation,
    ];
    let extracted = json_command(binary, state, &extract_args);
    assert_eq!(extracted["data"]["item_ids"][0]["id"], first);
    json_command(binary, state, &["thoughts", "undo", session]);

    exercise_merge(binary, state, session, first, second, &first_digest);

    let reflow_digest = thought_digest(binary, state, session, second);
    json_command(
        binary,
        state,
        &[
            "thoughts",
            "reflow",
            session,
            second,
            "--expected-sha256",
            &reflow_digest,
            "--operation-id",
            &operation_id(),
        ],
    );
    assert_thought_content(binary, state, session, second, "Keep me");
}

fn exercise_merge(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    first: &str,
    second: &str,
    first_digest: &str,
) {
    let second_digest = thought_digest(binary, state, session, second);
    std::fs::create_dir_all(state.join("config")).expect("create synthetic config directory");
    std::fs::write(
        state.join("config/config.toml"),
        "merge_separator = \"\\nchanged\\n\"\nmouse_capture = \"invalid\"\n",
    )
    .expect("invalidate unrelated caller settings after owner launch");
    let merge_operation = operation_id();
    let merge_args = [
        "thoughts",
        "merge",
        session,
        first,
        second,
        "--expected-sha256",
        first_digest,
        "--expected-sha256",
        &second_digest,
        "--operation-id",
        &merge_operation,
    ];
    let merged = json_command(binary, state, &merge_args);
    assert_eq!(merged["data"]["item_ids"].as_array().map(Vec::len), Some(2));
    assert_thought_content(
        binary,
        state,
        session,
        first,
        "external replacement\n\n  Keep\t me  ",
    );
    let merge_replay = json_command(binary, state, &merge_args);
    assert_eq!(merge_replay["data"]["receipt"]["idempotent_replay"], true);
    json_command(binary, state, &["thoughts", "undo", session]);
}

fn assert_additive_invalid_state_contract(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    thought: &str,
) {
    let legacy = raw_input_command(
        binary,
        state,
        &["thoughts", "move", session, thought, "99"],
        "",
    );
    assert!(!legacy.status.success());
    let legacy: serde_json::Value =
        serde_json::from_slice(&legacy.stdout).expect("legacy move JSON");
    assert_eq!(legacy["error"]["code"], "mutation_rejected");

    let typed = raw_input_command(
        binary,
        state,
        &["items", "move", session, thought, "99"],
        "",
    );
    assert!(!typed.status.success());
    let typed: serde_json::Value = serde_json::from_slice(&typed.stdout).expect("typed move JSON");
    assert_eq!(typed["error"]["code"], "invalid_state");
}

fn thought_digest(binary: &str, state: &std::path::Path, session: &str, thought: &str) -> String {
    json_command(binary, state, &["thoughts", "inspect", session, thought])["data"]["thought"]
        ["content_sha256"]
        .as_str()
        .expect("thought digest")
        .to_owned()
}
