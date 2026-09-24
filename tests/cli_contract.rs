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
fn current_capabilities_envelope_matches_the_checked_in_fixture() {
    assert_contract(
        include_str!("fixtures/cli/v1/capabilities.request.json"),
        include_str!("fixtures/cli/v1/capabilities.success.json"),
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
    assert_eq!(thoughts["items"][0]["id"], thought);
    assert_eq!(thoughts["items"][0]["content"], "keep me");
}

#[test]
fn json_help_and_version_are_successful_informational_responses() {
    let version = raw(&["--json", "--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(version.stderr.is_empty());
    let version: Value = serde_json::from_slice(&version.stdout).expect("version JSON");
    assert_eq!(version["schema_version"], 1);
    assert_eq!(version["ok"], true);
    assert_eq!(version["data"]["name"], "proqi");
    assert_eq!(version["data"]["version"], env!("CARGO_PKG_VERSION"));

    for arguments in [
        &["--json", "--help"][..],
        &["-h", "--json"][..],
        &["sessions", "ensure", "--help", "--json"][..],
    ] {
        let output = raw(arguments);
        assert_eq!(output.status.code(), Some(0), "{arguments:?}");
        let help: Value = serde_json::from_slice(&output.stdout).expect("help JSON");
        assert_eq!(help["ok"], true, "{arguments:?}");
        let text = help["data"]["help"].as_str().expect("help text");
        assert!(text.contains("Usage:"), "{arguments:?}");
        assert!(!text.contains('\u{1b}'), "help text is plain");
    }
    let ensure = raw(&["sessions", "ensure", "--help", "--json"]);
    let ensure: Value = serde_json::from_slice(&ensure.stdout).expect("help JSON");
    assert!(
        ensure["data"]["help"]
            .as_str()
            .expect("help")
            .contains("--cwd")
    );

    let human = raw(&["--version"]);
    assert_eq!(human.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(human.stdout).expect("UTF-8"),
        format!("proqi {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn only_a_parsed_json_option_selects_machine_errors() {
    let positional = raw(&["sessions", "rename", "--", "--json"]);
    assert_eq!(positional.status.code(), Some(2));
    assert!(
        positional.stdout.is_empty(),
        "positional --json stays human"
    );
    assert!(!positional.stderr.is_empty());

    let option = raw(&["--json", "sessions", "rename"]);
    assert_eq!(option.status.code(), Some(2));
    let error: Value = serde_json::from_slice(&option.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "invalid_arguments");
}

fn raw(arguments: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args(arguments)
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .expect("run proqi")
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
