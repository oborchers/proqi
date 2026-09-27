//! Real-binary `thoughts capture --from clipboard` on an inactive session.
//!
//! Every invocation names a clipboard fixture, so no test reads or overwrites
//! the user's real clipboard.

use std::{
    path::Path,
    process::{Command, Output, Stdio},
};

use serde_json::Value;

use super::{create_session, operation_id, success};

fn capture(root: &Path, fixture: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(root)
        .arg("--json")
        .args(["thoughts", "capture"])
        .args(arguments)
        .env_remove("HERDR_ENV")
        .env("PROQI_TEST_CLIPBOARD_FIXTURE", fixture)
        .stdin(Stdio::null())
        .output()
        .expect("run proqi")
}

fn clipboard(directory: &Path, fixture: &Value) -> std::path::PathBuf {
    let path = directory.join("clipboard.json");
    std::fs::write(&path, fixture.to_string()).expect("clipboard fixture");
    path
}

fn text(directory: &Path, text: &str) -> std::path::PathBuf {
    clipboard(
        directory,
        &serde_json::json!({ "kind": "text", "text": text }),
    )
}

fn captured(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    value["data"].clone()
}

fn failure(output: &Output) -> Value {
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(7));
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(value["ok"], false);
    value["error"].clone()
}

fn contents(root: &Path, session: &str) -> Vec<String> {
    success(root, &["thoughts", "list", session], None)["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["content"].as_str().expect("content").to_owned())
        .collect()
}

#[test]
fn clipboard_text_is_appended_exactly_as_one_undoable_thought() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    let session = create_session(&root);
    success(&root, &["thoughts", "add", &session], Some("existing"));
    let body = "  Grüße 👩‍💻\r\n\tline two\u{1b}[0m \n";
    let fixture = text(temporary.path(), body);

    let data = captured(&capture(
        &root,
        &fixture,
        &[&session, "--from", "clipboard"],
    ));
    assert_eq!(data["source"], "clipboard");
    assert_eq!(data["bytes"], body.len());
    assert_eq!(data["characters"], 25);
    assert_eq!(data["receipt"]["idempotent_replay"], false);
    assert_eq!(contents(&root, &session), vec!["existing", body]);

    success(&root, &["thoughts", "undo", &session], None);
    assert_eq!(contents(&root, &session), vec!["existing"]);
}

#[test]
fn the_same_text_captured_twice_creates_two_thoughts() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    let session = create_session(&root);
    let fixture = text(temporary.path(), "same");
    for _ in 0..2 {
        captured(&capture(
            &root,
            &fixture,
            &[&session, "--from", "clipboard"],
        ));
    }
    assert_eq!(contents(&root, &session), vec!["same", "same"]);
}

#[test]
fn an_operation_identity_replays_like_thoughts_add() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    let session = create_session(&root);
    let fixture = text(temporary.path(), "first capture");
    let operation = operation_id();
    let arguments = [
        session.as_str(),
        "--from",
        "clipboard",
        "--operation-id",
        &operation,
    ];
    let first = captured(&capture(&root, &fixture, &arguments));
    let replay = captured(&capture(&root, &fixture, &arguments));
    assert_eq!(replay["thought_id"], first["thought_id"]);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    assert_eq!(contents(&root, &session), vec!["first capture"]);

    let changed = text(temporary.path(), "different clipboard");
    let error = failure(&capture(&root, &changed, &arguments));
    assert_eq!(error["code"], "idempotency_conflict");
    assert_eq!(contents(&root, &session), vec!["first capture"]);

    // The same identity also replays through thoughts add with the same text.
    let add = success(
        &root,
        &["thoughts", "add", &session, "--operation-id", &operation],
        Some("first capture"),
    );
    assert_eq!(add["receipt"]["idempotent_replay"], true);
}

#[test]
fn empty_non_text_and_oversized_clipboards_store_nothing_with_stable_codes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    let session = create_session(&root);
    let oversized = "x".repeat(128 * 1024 + 1);
    let cases = [
        (
            serde_json::json!({ "kind": "text", "text": " \r\n\t" }),
            "capture_empty",
            "Nothing captured: the clipboard is empty",
            serde_json::json!({ "source": "clipboard" }),
        ),
        (
            serde_json::json!({ "kind": "image" }),
            "capture_no_text",
            "Nothing captured: the clipboard holds no text (for example an image)",
            serde_json::json!({ "source": "clipboard" }),
        ),
        (
            serde_json::json!({ "kind": "text", "text": oversized }),
            "capture_too_large",
            "Nothing captured: the clipboard has 131073 bytes, more than the 131072-byte thought limit",
            serde_json::json!({ "source": "clipboard", "bytes": 131_073, "limit": 131_072 }),
        ),
    ];
    for (fixture, code, message, details) in cases {
        let path = clipboard(temporary.path(), &fixture);
        let error = failure(&capture(&root, &path, &[&session, "--from", "clipboard"]));
        assert_eq!(error["code"], code);
        assert_eq!(error["message"], message);
        assert_eq!(error["details"], details);
    }
    let limit = text(temporary.path(), &"y".repeat(128 * 1024));
    captured(&capture(&root, &limit, &[&session, "--from", "clipboard"]));
    let stored = contents(&root, &session);
    assert_eq!(stored.len(), 1, "only the in-limit capture was stored");
    assert_eq!(stored[0].len(), 128 * 1024);
}

#[test]
fn an_unreadable_clipboard_fails_as_clipboard_failed() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    let session = create_session(&root);
    let fixture = clipboard(
        temporary.path(),
        &serde_json::json!({ "kind": "unavailable" }),
    );
    let output = capture(&root, &fixture, &[&session, "--from", "clipboard"]);
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(error["error"]["code"], "clipboard_failed");
    assert!(contents(&root, &session).is_empty());
}

#[test]
fn an_unknown_source_or_session_stores_nothing() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    let session = create_session(&root);
    let fixture = text(temporary.path(), "text");
    let output = capture(&root, &fixture, &[&session, "--from", "selection"]);
    assert_eq!(output.status.code(), Some(2));
    let missing = capture(
        &root,
        &fixture,
        &["ses_06g30t7dv5qv55n1ppn3clis3k", "--from", "clipboard"],
    );
    let error: Value = serde_json::from_slice(&missing.stdout).expect("JSON");
    assert_eq!(error["error"]["code"], "not_found");
    assert!(contents(&root, &session).is_empty());
}
