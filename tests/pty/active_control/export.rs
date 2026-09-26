//! Thought export through one real active TUI owner over owner control.

use std::fs;

use super::{assert_thought_content, operation_id, spawn_owner};
use crate::support::{json_command, json_input_command, wait_for_control_owner, wait_for_path};

#[test]
fn active_owner_commits_the_export_board_step_and_replays_without_rewriting() {
    let state = tempfile::tempdir().expect("temporary state");
    let files = tempfile::tempdir().expect("export directory");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let add = |content: &str| {
        json_input_command(binary, state.path(), &["thoughts", "add", session], content)
            ["data"]["thought_id"]
            .as_str()
            .expect("thought ID")
            .to_owned()
    };
    let first = add("owned first");
    let second = add("owned second");
    let ready = state.path().join("export-owner-ready");
    let done = state.path().join("export-owner-done");
    let mut owner = spawn_owner(binary, state.path(), session, &ready, &done);
    wait_for_path(&ready);
    wait_for_control_owner(state.path(), session);

    export_and_replay(
        binary,
        state.path(),
        session,
        [&first, &second],
        files.path(),
    );
    json_command(
        binary,
        state.path(),
        &[
            "thoughts",
            "undo",
            session,
            "--operation-id",
            &operation_id(),
        ],
    );
    assert_thought_content(binary, state.path(), session, &first, "owned first");
    assert_thought_content(binary, state.path(), session, &second, "owned second");

    fs::write(&done, b"done").expect("release export owner workflow");
    let status = owner.wait().expect("wait for export owner workflow");
    assert!(status.success(), "export owner PTY exited with {status}");
    let items = json_command(binary, state.path(), &["thoughts", "list", session]);
    let contents = items["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["content"].as_str().expect("content").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(contents, ["owned first", "owned second"]);
}

/// Export through the owner, then prove an exact retry replays without rewriting.
fn export_and_replay(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    [first, second]: [&str; 2],
    files: &std::path::Path,
) {
    let output = files.join("owned.txt");
    let output_text = output.to_str().expect("UTF-8 path");
    let operation = operation_id();
    let arguments = [
        "thoughts",
        "export",
        session,
        second,
        first,
        "--output",
        output_text,
        "--replace-with-reference",
        "--operation-id",
        &operation,
    ];
    let exported = json_command(binary, state, &arguments);
    assert_eq!(exported["ok"], true, "{exported}");
    let reference = exported["data"]["reference_thought_id"]
        .as_str()
        .expect("reference")
        .to_owned();
    assert_eq!(exported["data"]["receipt"]["idempotent_replay"], false);
    assert_eq!(
        fs::read_to_string(&output).expect("file"),
        "owned first\n\nowned second"
    );
    assert_thought_content(
        binary,
        state,
        session,
        &reference,
        &format!("{output_text} "),
    );

    fs::write(&output, "edited after export").expect("user edit");
    let replay = json_command(binary, state, &arguments);
    assert_eq!(
        replay["data"]["receipt"]["idempotent_replay"], true,
        "{replay}"
    );
    assert_eq!(replay["data"]["reference_thought_id"], reference.as_str());
    assert_eq!(
        fs::read_to_string(&output).expect("file"),
        "edited after export",
        "a replay never rewrites the file"
    );
}
