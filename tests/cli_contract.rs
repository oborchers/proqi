//! Checked-in examples of the installed version's current JSON CLI contract.
//!
//! These fixtures detect accidental drift. Proqi does not promise pre-1.0
//! compatibility with fixtures from another installed minor release.

use std::{
    io::Write as _,
    path::Path,
    process::{Output, Stdio},
};

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator as _};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct RequestFixture {
    arguments: Vec<String>,
    stdin: Option<String>,
}

#[test]
fn current_success_envelope_matches_the_checked_in_fixture() {
    assert_contract(
        include_str!("fixtures/cli/v1/sessions_list.request.json"),
        include_str!("fixtures/cli/v1/sessions_list.success.json"),
        true,
    );
}

#[test]
fn current_error_envelope_matches_the_checked_in_fixture() {
    assert_contract(
        include_str!("fixtures/cli/v1/wrong_session_prefix.request.json"),
        include_str!("fixtures/cli/v1/wrong_session_prefix.error.json"),
        false,
    );
}

#[test]
fn one_operation_identity_cannot_be_reused_for_another_mutation_kind() {
    let state = tempfile::tempdir().expect("temporary state");
    let created = success(state.path(), &[], None);
    let session = created["session_id"].as_str().expect("session ID");
    let operation = SystemIdGenerator.operation_id().to_string();
    let added = success(
        state.path(),
        &["thoughts", "add", session, "--operation-id", &operation],
        Some("keep me"),
    );
    let thought = added["thought_id"].as_str().expect("thought ID");

    let reused = run(
        state.path(),
        &[
            "thoughts",
            "delete",
            session,
            thought,
            "--operation-id",
            &operation,
        ],
        None,
    );

    assert!(!reused.status.success());
    let error: Value = serde_json::from_slice(&reused.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "idempotency_conflict");
    let thoughts = success(state.path(), &["thoughts", "list", session], None);
    assert_eq!(thoughts["thoughts"][0]["id"], thought);
    assert_eq!(thoughts["thoughts"][0]["content"], "keep me");
}

fn assert_contract(request: &str, expected: &str, succeeds: bool) {
    let request: RequestFixture = serde_json::from_str(request).expect("request fixture");
    assert!(
        request
            .arguments
            .iter()
            .any(|argument| argument == "--json")
    );
    let state = tempfile::tempdir().expect("temporary state");
    let arguments = request
        .arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let output = run(state.path(), &arguments, request.stdin.as_deref());
    assert_eq!(output.status.success(), succeeds);
    assert!(output.stderr.is_empty());
    let actual: Value = serde_json::from_slice(&output.stdout).expect("CLI JSON");
    let expected: Value = serde_json::from_str(expected).expect("response fixture");
    assert_eq!(actual, expected);
    assert_eq!(actual["schema_version"], 1);
}

fn success(state: &Path, arguments: &[&str], input: Option<&str>) -> Value {
    let output = run(state, arguments, input);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("success JSON");
    assert_eq!(response["schema_version"], 1);
    assert_eq!(response["ok"], true);
    response["data"].clone()
}

fn run(state: &Path, arguments: &[&str], input: Option<&str>) -> Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_proqi"));
    command
        .arg("--state-dir")
        .arg(state)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    if !arguments.contains(&"--json") {
        command.arg("--json");
    }
    command.args(arguments);
    let mut child = command.spawn().expect("start CLI command");
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("command stdin")
            .write_all(input.as_bytes())
            .expect("write command stdin");
    }
    child.wait_with_output().expect("finish CLI command")
}
