//! Cross-process contract for the private bounded attachment checker.

use std::{
    fs,
    io::Write as _,
    process::{Command, Stdio},
};

#[test]
fn hidden_worker_checks_unicode_and_missing_paths_without_runtime_state() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let accessible = temporary.path().join("Grüße 第一.txt");
    fs::write(&accessible, b"proof").expect("accessible fixture");
    let missing = temporary.path().join("missing.txt");
    let request = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "paths": [accessible, missing],
    }))
    .expect("worker request");

    let mut child = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(temporary.path().join("worker-state"))
        .arg("__attachment-check")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn worker");
    child
        .stdin
        .take()
        .expect("worker stdin")
        .write_all(&request)
        .expect("write request");
    let output = child.wait_with_output().expect("worker output");
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("response JSON");
    assert_eq!(response["version"], 1);
    assert_eq!(response["failures"][0], serde_json::Value::Null);
    assert_eq!(response["failures"][1], "missing");
    assert!(!temporary.path().join("worker-state").exists());
}
