//! Retry identity after later edits and refusal of non-UTF-8 destinations.

use std::{
    fs,
    process::{Command, Stdio},
};

use serde_json::Value;

use super::{add, failure, live_contents};
use crate::{create_session, operation_id, success};

#[test]
fn exact_retries_replay_after_later_edits_and_other_destinations_conflict() {
    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let thought = add(root, &session, "before");
    let output = files.path().join("removed.txt");
    let output_text = output.to_str().expect("path");
    let operation = operation_id();
    let arguments = [
        "thoughts",
        "export",
        &session,
        &thought,
        "--output",
        output_text,
        "--remove",
        "--operation-id",
        &operation,
    ];
    let first = success(root, &arguments, None);
    success(root, &["thoughts", "undo", &session], None);
    let digest = success(root, &["thoughts", "inspect", &session, &thought], None)["thought"]
        ["content_sha256"]
        .as_str()
        .expect("digest")
        .to_owned();
    success(
        root,
        &[
            "thoughts",
            "replace",
            &session,
            &thought,
            "--expected-sha256",
            &digest,
        ],
        Some("after an edit"),
    );
    let replay = success(root, &arguments, None);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    assert_eq!(
        replay["receipt"]["operation_id"],
        first["receipt"]["operation_id"]
    );
    assert_eq!(fs::read_to_string(&output).expect("file"), "before");
    assert_eq!(live_contents(root, &session), ["after an edit"]);

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
            "--remove",
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

#[test]
fn non_utf8_destinations_are_refused_before_writing() {
    use std::os::unix::ffi::OsStrExt as _;

    let state = tempfile::tempdir().expect("state");
    let files = tempfile::tempdir().expect("files");
    let root = state.path();
    let session = create_session(root);
    let thought = add(root, &session, "body");
    let odd = files.path().join(std::ffi::OsStr::from_bytes(b"caf\xe9"));
    if fs::create_dir(&odd).is_err() {
        return;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .current_dir(&odd)
        .arg("--state-dir")
        .arg(root)
        .args([
            "--json",
            "thoughts",
            "export",
            &session,
            &thought,
            "--output",
            "x.txt",
            "--replace-with-reference",
        ])
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .expect("run proqi");
    let error: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(error["error"]["code"], "export_target_invalid");
    assert_eq!(error["error"]["details"]["reason"], "not_utf8");
    assert_eq!(fs::read_dir(&odd).expect("list").count(), 0);
    assert_eq!(live_contents(root, &session), ["body"]);
}
