//! Real-binary contracts for the scriptable session and thought workflow.

use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use proqi::{
    adapters::runtime::{FileRuntimeCoordinator, SystemIdGenerator},
    domain::{SessionId, Timestamp},
    ports::{environment::IdGenerator, runtime::RuntimeCoordinator},
};
use serde_json::Value;

fn run(root: &Path, arguments: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_proqi"));
    command
        .arg("--state-dir")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn().expect("spawn proqi");
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("child stdin")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    child.wait_with_output().expect("wait for proqi")
}

fn success(root: &Path, arguments: &[&str], input: Option<&str>) -> Value {
    let output = run(root, arguments, input);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON response");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["ok"], true);
    value["data"].clone()
}

fn create_session(root: &Path) -> String {
    success(root, &[], None)["session_id"]
        .as_str()
        .expect("session ID")
        .to_owned()
}

fn operation_id() -> String {
    SystemIdGenerator.operation_id().to_string()
}

fn add_with_idempotency(root: &Path, session: &str, body: &str) -> String {
    let operation = operation_id();
    let arguments = ["thoughts", "add", session, "--operation-id", &operation];
    let added = success(root, &arguments, Some(body));
    let thought = added["thought_id"].as_str().expect("thought ID").to_owned();
    assert_eq!(added["receipt"]["idempotent_replay"], false);

    let replay = success(root, &arguments, Some(body));
    assert_eq!(replay["thought_id"], thought);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    let conflict = run(root, &arguments, Some("different"));
    assert!(!conflict.status.success());
    let error: Value = serde_json::from_slice(&conflict.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "idempotency_conflict");
    thought
}

#[test]
fn session_management_is_searchable_recoverable_and_explicitly_prunable() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let first = create_session(root);
    let second = create_session(root);

    let renamed = success(root, &["sessions", "rename", &first, "research"], None);
    assert_eq!(renamed["status"], "renamed");
    let listed = success(root, &["sessions", "list", "--query", "research"], None);
    assert_eq!(listed["sessions"].as_array().expect("sessions").len(), 1);
    assert_eq!(listed["sessions"][0]["id"], first);

    success(root, &["sessions", "trash", "research"], None);
    let visible = success(root, &["sessions", "list", "--all"], None);
    let trashed = visible["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|session| session["id"] == first)
        .expect("trashed session");
    assert_eq!(trashed["state"], "trashed");
    success(root, &["sessions", "restore", "research"], None);
    success(root, &["sessions", "rename", &first, "--clear"], None);
    let unnamed = success(root, &["sessions", "list"], None);
    let cleared = unnamed["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .find(|session| session["id"] == first)
        .expect("renamed session");
    assert!(cleared["name"].is_null());
    success(root, &["sessions", "rename", &first, "research"], None);
    success(root, &["sessions", "trash", "research"], None);

    let refused = run(root, &["sessions", "prune", "research"], None);
    assert!(!refused.status.success());
    let error: Value = serde_json::from_slice(&refused.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "invalid_arguments");
    success(root, &["sessions", "prune", "research", "--yes"], None);

    let remaining = success(root, &["sessions"], None);
    assert_eq!(remaining["sessions"].as_array().expect("sessions").len(), 1);
    assert_eq!(remaining["sessions"][0]["id"], second);
}

#[test]
fn thought_mutations_round_trip_unicode_and_idempotency_across_processes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let body = "Grüße 👩‍💻\n第二行\n";
    let thought = add_with_idempotency(root, &session, body);
    let content_search = success(root, &["sessions", "list", "--query", "Grüße"], None);
    assert_eq!(content_search["sessions"][0]["id"], session);

    let inspected = success(root, &["thoughts", "inspect", &session, &thought], None);
    assert_eq!(inspected["thought"]["content"], body);
    let second = success(root, &["thoughts", "add", &session], Some("second"));
    let second_id = second["thought_id"].as_str().expect("second thought");
    let moved = success(
        root,
        &[
            "thoughts",
            "move",
            &session,
            &thought,
            "1",
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    assert_eq!(moved["thought_id"], thought);
    let reordered = success(root, &["thoughts", "list", &session], None);
    assert_eq!(reordered["thoughts"][0]["id"], second_id);
    assert_eq!(reordered["thoughts"][1]["id"], thought);
    let delete = operation_id();
    success(
        root,
        &[
            "thoughts",
            "delete",
            &session,
            &thought,
            "--operation-id",
            &delete,
        ],
        None,
    );
    let after_delete = success(root, &["thoughts", "list", &session], None);
    assert_eq!(
        after_delete["thoughts"].as_array().expect("thoughts").len(),
        1
    );
    assert_eq!(after_delete["thoughts"][0]["id"], second_id);
    let undo = operation_id();
    let undone = success(
        root,
        &["thoughts", "undo", &session, "--operation-id", &undo],
        None,
    );
    assert_eq!(undone["receipt"]["idempotent_replay"], false);
    let replayed_undo = success(
        root,
        &["thoughts", "undo", &session, "--operation-id", &undo],
        None,
    );
    assert_eq!(replayed_undo["receipt"]["idempotent_replay"], true);
    let restored = success(root, &["thoughts", "list", &session], None);
    assert_eq!(restored["thoughts"][0]["id"], second_id);
    assert_eq!(restored["thoughts"][1]["content"], body);
    success(root, &["thoughts", "redo", &session], None);
    let after_redo = success(root, &["thoughts", "list", &session], None);
    assert_eq!(
        after_redo["thoughts"].as_array().expect("thoughts").len(),
        1
    );
    assert_eq!(after_redo["thoughts"][0]["id"], second_id);
}

#[test]
fn launch_modes_and_capability_discovery_have_stable_output() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let created = success(root, &[], None);
    let session = created["session_id"].as_str().expect("session ID");
    assert_eq!(created["resume_command"], format!("proqi -r {session}"));

    let continued = success(root, &["-c"], None);
    assert_eq!(continued["session_id"], session);
    let resumed = success(root, &["-r", session], None);
    assert_eq!(resumed["session_id"], session);
    let capabilities = success(root, &["capabilities"], None);
    assert_eq!(capabilities["cli_schema_version"], 1);
    assert_eq!(capabilities["active_session_control"], true);
    assert_eq!(capabilities["herdr_submission"], false);

    let non_terminal = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(root)
        .output()
        .expect("non-terminal launch");
    assert!(!non_terminal.status.success());
    let text = String::from_utf8(non_terminal.stderr).expect("UTF-8 output");
    assert!(text.contains("interactive launch requires a terminal"));
    let after_failure = success(root, &["sessions", "list"], None);
    assert_eq!(
        after_failure["sessions"]
            .as_array()
            .expect("sessions")
            .len(),
        1
    );
}

#[test]
fn newer_schema_is_reported_as_unsupported() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    create_session(root);
    let connection =
        rusqlite::Connection::open(root.join("data/proqi.sqlite3")).expect("open database fixture");
    connection
        .execute("UPDATE schema_meta SET schema_version = 999", [])
        .expect("advance schema fixture");
    drop(connection);

    let output = run(root, &["sessions"], None);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "unsupported");
}

#[test]
fn names_can_be_ambiguous_and_identifier_prefixes_are_strict() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let first = create_session(root);
    let second = create_session(root);
    success(root, &["sessions", "rename", &first, "same"], None);
    success(root, &["sessions", "rename", &second, "same"], None);

    let ambiguous = run(root, &["-r", "same"], None);
    assert!(!ambiguous.status.success());
    let error: Value = serde_json::from_slice(&ambiguous.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "ambiguous_session");
    assert_eq!(
        error["error"]["details"]["matches"]
            .as_array()
            .expect("matches")
            .len(),
        2
    );

    let wrong_prefix = first.replacen("ses_", "tht_", 1);
    let rejected = run(root, &["thoughts", "list", &wrong_prefix], None);
    assert!(!rejected.status.success());
    let error: Value = serde_json::from_slice(&rejected.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "invalid_identifier");
}

#[test]
fn active_session_conflict_is_structured_and_nonzero() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let before = success(root, &["sessions", "list"], None);
    let opened_before = before["sessions"][0]["last_opened_at"]
        .as_i64()
        .expect("opening timestamp");
    let session_id: SessionId = session.parse().expect("session ID");
    let mut ids = SystemIdGenerator;
    let coordinator = FileRuntimeCoordinator::new(
        root.join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        Timestamp::from_millis(1),
        "test-owner",
    )
    .expect("runtime coordinator");
    let _lease = coordinator
        .acquire_session(session_id)
        .expect("owner lease");

    let output = run(root, &["-r", &session], None);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "session_busy");
    assert_eq!(error["error"]["details"]["session_id"], session);
    let mutation = run(root, &["thoughts", "add", &session], Some("blocked"));
    assert!(!mutation.status.success());
    let mutation_error: Value =
        serde_json::from_slice(&mutation.stdout).expect("mutation error JSON");
    assert_eq!(mutation_error["error"]["code"], "session_busy");
    let after = success(root, &["sessions", "list"], None);
    assert_eq!(after["sessions"][0]["last_opened_at"], opened_before);
}
