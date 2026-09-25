//! Real-binary `thoughts export` content, overwrite, durability, undo, and replay contracts.

use std::{
    fs,
    os::unix::fs::{PermissionsExt as _, symlink},
    path::Path,
    process::{Command, Stdio},
};

use serde_json::Value;

use super::{create_session, operation_id, run, success};

fn add(root: &Path, session: &str, body: &str) -> String {
    success(root, &["thoughts", "add", session], Some(body))["thought_id"]
        .as_str()
        .expect("thought ID")
        .to_owned()
}

fn failure(root: &Path, arguments: &[&str]) -> (i32, Value) {
    let output = run(root, arguments, None);
    assert!(!output.status.success(), "command unexpectedly succeeded");
    let value: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    (
        output.status.code().expect("exit status"),
        value["error"].clone(),
    )
}

fn live_contents(root: &Path, session: &str) -> Vec<String> {
    success(root, &["thoughts", "list", session], None)["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|item| item["kind"] == "thought")
        .map(|item| item["content"].as_str().expect("content").to_owned())
        .collect()
}

#[test]
fn keep_writes_exact_copy_text_in_board_order_and_never_overwrites() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let first = add(root, &session, "Grüße 👩‍💻\r\n\ttabs  ");
    let second = add(root, &session, "\n\nsecond\n");
    let output = files.path().join("notes.md");
    let output_text = output.to_str().expect("UTF-8 path");
    let data = success(
        root,
        &[
            "thoughts",
            "export",
            &session,
            &second,
            &first,
            "--output",
            output_text,
        ],
        None,
    );
    let expected = "Grüße 👩‍💻\r\n\ttabs  \n\n\n\nsecond\n";
    assert_eq!(fs::read(&output).expect("file"), expected.as_bytes());
    assert_eq!(data["disposition"], "keep");
    assert_eq!(data["bytes"], expected.len());
    assert_eq!(data["thought_ids"], serde_json::json!([first, second]));
    assert_eq!(data["receipt"], Value::Null);
    assert_eq!(data["written"], true);
    assert_eq!(live_contents(root, &session).len(), 2);

    fs::write(&output, "keep me").expect("existing file");
    let (exit, error) = failure(
        root,
        &[
            "thoughts",
            "export",
            &session,
            &first,
            "--output",
            output_text,
        ],
    );
    assert_eq!(
        (exit, error["code"].as_str()),
        (7, Some("export_target_exists"))
    );
    assert_eq!(error["details"]["output"], output_text);
    assert_eq!(fs::read_to_string(&output).expect("file"), "keep me");

    let replaced = success(
        root,
        &[
            "thoughts",
            "export",
            &session,
            &first,
            "--output",
            output_text,
            "--replace-existing",
        ],
        None,
    );
    assert_eq!(replaced["replaced"], true);
    assert_eq!(
        fs::read_to_string(&output).expect("file"),
        "Grüße 👩‍💻\r\n\ttabs  "
    );
}

#[test]
fn relative_output_resolves_from_the_process_directory() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let thought = add(root, &session, "relative");
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .current_dir(files.path())
        .args(["--state-dir"])
        .arg(root)
        .args([
            "--json",
            "thoughts",
            "export",
            &session,
            &thought,
            "--output",
            "./sub/../x.txt",
        ])
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .expect("run proqi");
    let _ = fs::create_dir(files.path().join("sub"));
    assert!(!output.status.success(), "missing sub directory is refused");
    let error: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(error["error"]["code"], "export_directory_missing");
    assert!(!files.path().join("x.txt").exists());

    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .current_dir(files.path())
        .arg("--state-dir")
        .arg(root)
        .args([
            "--json",
            "thoughts",
            "export",
            &session,
            &thought,
            "--output",
            "./sub/../x.txt",
        ])
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .expect("run proqi");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(files.path().join("x.txt")).expect("file"),
        "relative"
    );
}

fn export_failure(
    root: &Path,
    session: &str,
    thought: &str,
    output: &Path,
    flags: &[&str],
) -> (i32, Value) {
    let output = output.to_str().expect("UTF-8 path");
    let mut arguments = vec!["thoughts", "export", session, thought, "--output", output];
    arguments.extend_from_slice(flags);
    failure(root, &arguments)
}

#[test]
fn links_folders_and_conflicting_flags_are_refused_before_any_write() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let thought = add(root, &session, "body");
    let target = files.path().join("target.txt");
    fs::write(&target, "target").expect("target");
    let link = files.path().join("link.txt");
    symlink(&target, &link).expect("link");
    let (exit, error) = export_failure(
        root,
        &session,
        &thought,
        &link,
        &["--replace-existing", "--remove"],
    );
    assert_eq!(
        (exit, error["code"].as_str()),
        (2, Some("export_target_invalid"))
    );
    assert_eq!(error["details"]["reason"], "target_is_symlink");
    assert_eq!(fs::read_to_string(&target).expect("target"), "target");
    assert!(
        fs::symlink_metadata(&link)
            .expect("link")
            .file_type()
            .is_symlink()
    );

    let (exit, error) = export_failure(root, &session, &thought, files.path(), &[]);
    assert_eq!(
        (exit, error["code"].as_str()),
        (2, Some("export_target_invalid"))
    );
    assert_eq!(error["details"]["reason"], "target_is_directory");
    let (exit, error) = export_failure(
        root,
        &session,
        &thought,
        &files.path().join("x.txt"),
        &["--remove", "--replace-with-reference"],
    );
    assert_eq!(
        (exit, error["code"].as_str()),
        (2, Some("invalid_arguments"))
    );
    assert_eq!(live_contents(root, &session), ["body"]);
}

#[test]
fn missing_and_read_only_folders_fail_without_board_changes() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let thought = add(root, &session, "body");
    let missing = files.path().join("missing/out.txt");
    let (exit, error) = export_failure(root, &session, &thought, &missing, &["--remove"]);
    assert_eq!(
        (exit, error["code"].as_str()),
        (3, Some("export_directory_missing"))
    );
    assert!(!files.path().join("missing").exists(), "never created");

    let locked = files.path().join("locked");
    fs::create_dir(&locked).expect("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("read-only");
    let result = export_failure(
        root,
        &session,
        &thought,
        &locked.join("out.txt"),
        &["--remove"],
    );
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("restore");
    assert_eq!(
        (result.0, result.1["code"].as_str()),
        (1, Some("export_write_failed"))
    );
    assert_eq!(result.1["details"]["reason"], "permission_denied");
    assert_eq!(fs::read_dir(&locked).expect("list").count(), 0);
    assert_eq!(live_contents(root, &session), ["body"]);
}

#[test]
fn remove_and_replace_are_single_undo_steps_that_keep_the_file() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let first = add(root, &session, "first");
    let second = add(root, &session, "second");
    let removed = files.path().join("removed.txt");
    let data = success(
        root,
        &[
            "thoughts",
            "export",
            &session,
            &first,
            &second,
            "--output",
            removed.to_str().expect("path"),
            "--remove",
        ],
        None,
    );
    assert_eq!(data["disposition"], "remove");
    assert!(live_contents(root, &session).is_empty());
    success(root, &["thoughts", "undo", &session], None);
    assert_eq!(live_contents(root, &session), ["first", "second"]);
    assert_eq!(
        fs::read_to_string(&removed).expect("kept"),
        "first\n\nsecond"
    );

    let reference_path = files.path().join("Ref file.txt");
    let reference_text = reference_path.to_str().expect("path");
    let data = success(
        root,
        &[
            "thoughts",
            "export",
            &session,
            &second,
            "--output",
            reference_text,
            "--replace-with-reference",
        ],
        None,
    );
    let reference = data["reference_thought_id"].as_str().expect("reference");
    assert_eq!(
        live_contents(root, &session),
        ["first".to_owned(), format!("{reference_text} ")]
    );
    let inspected = success(root, &["thoughts", "inspect", &session, reference], None);
    assert_eq!(
        inspected["thought"]["content"],
        format!("{reference_text} ")
    );
    success(root, &["thoughts", "undo", &session], None);
    assert_eq!(live_contents(root, &session), ["first", "second"]);
    assert_eq!(fs::read_to_string(&reference_path).expect("kept"), "second");
}

#[test]
fn operation_identity_replays_without_rewriting_and_converges_after_interruption() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let thought = add(root, &session, "exported once");
    let output = files.path().join("once.txt");
    let output_text = output.to_str().expect("path");
    fs::write(&output, "exported once").expect("interrupted earlier write");
    let operation = operation_id();
    let arguments = [
        "thoughts",
        "export",
        &session,
        &thought,
        "--output",
        output_text,
        "--replace-with-reference",
        "--operation-id",
        &operation,
    ];
    let first = success(root, &arguments, None);
    assert_eq!(
        first["written"], false,
        "identical bytes converge without rewriting"
    );
    assert_eq!(first["receipt"]["idempotent_replay"], false);
    fs::write(&output, "edited by the user afterwards").expect("later edit");
    let replay = success(root, &arguments, None);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    assert_eq!(
        replay["reference_thought_id"],
        first["reference_thought_id"]
    );
    assert_eq!(replay["written"], false);
    assert_eq!(
        fs::read_to_string(&output).expect("file"),
        "edited by the user afterwards",
        "a replay never rewrites the file"
    );
    let other = files.path().join("other.txt");
    let (exit, error) = failure(
        root,
        &[
            "thoughts",
            "export",
            &session,
            &thought,
            "--output",
            other.to_str().expect("path"),
            "--replace-with-reference",
            "--operation-id",
            &operation,
        ],
    );
    assert_eq!(
        (exit, error["code"].as_str()),
        (7, Some("idempotency_conflict"))
    );
    assert!(!other.exists());
}

#[path = "export/retries.rs"]
mod retries;
