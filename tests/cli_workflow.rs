//! Real-binary contracts for the scriptable session and thought workflow.

use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use proqi::{adapters::runtime::SystemIdGenerator, ports::environment::IdGenerator};
use serde_json::Value;

#[path = "cli_workflow/diagnostics.rs"]
mod diagnostics;
#[path = "cli_workflow/doctor.rs"]
mod doctor;
#[path = "cli_workflow/session_contract.rs"]
mod session_contract;

fn run(root: &Path, arguments: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_proqi"));
    command
        .arg("--state-dir")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .env_remove("HERDR_ENV")
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

fn revision_id() -> String {
    SystemIdGenerator.revision_id().to_string()
}

fn rename(root: &Path, session: &str, name: &str) {
    let _renamed = success(root, &["sessions", "rename", session, name], None);
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
fn exact_replacement_requires_a_precondition_and_uses_editor_history() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let added = success(root, &["thoughts", "add", &session], Some("before"));
    let thought = added["thought_id"].as_str().expect("thought ID");
    let inspected = success(root, &["thoughts", "inspect", &session, thought], None);
    let digest = inspected["thought"]["content_sha256"]
        .as_str()
        .expect("digest");
    let revision = revision_id();

    let replaced = exact_replace(root, &session, thought, &revision, digest, "replacement");
    assert_eq!(replaced["receipt"]["idempotent_replay"], false);
    let replay = exact_replace(root, &session, thought, &revision, digest, "replacement");
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    let stale = run(
        root,
        &[
            "thoughts",
            "replace",
            &session,
            thought,
            "--expected-sha256",
            digest,
        ],
        Some("stale"),
    );
    assert!(!stale.status.success());
    let error: Value = serde_json::from_slice(&stale.stdout).expect("conflict JSON");
    assert_eq!(error["error"]["code"], "content_conflict");
    success(
        root,
        &["thoughts", "replace", &session, thought, "--force"],
        Some("forced"),
    );
    success(
        root,
        &["thoughts", "undo", &session, "--thought", thought],
        None,
    );
    let restored = success(root, &["thoughts", "inspect", &session, thought], None);
    assert_eq!(restored["thought"]["content"], "replacement");

    success(
        root,
        &[
            "thoughts",
            "collapse",
            &session,
            thought,
            "--collapsed",
            "true",
        ],
        None,
    );
    let collapsed = success(root, &["thoughts", "inspect", &session, thought], None);
    assert_eq!(collapsed["thought"]["collapsed"], true);
}

fn exact_replace(
    root: &Path,
    session: &str,
    thought: &str,
    revision: &str,
    digest: &str,
    content: &str,
) -> Value {
    success(
        root,
        &[
            "thoughts",
            "replace",
            session,
            thought,
            "--revision-id",
            revision,
            "--expected-sha256",
            digest,
        ],
        Some(content),
    )
}

#[test]
fn thoughts_copy_between_named_sessions_and_remove_only_after_delivery() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let source = create_session(root);
    let destination = create_session(root);
    rename(root, &source, "source");
    rename(root, &destination, "destination");
    let added = success(root, &["thoughts", "add", "source"], Some("exact\n内容"));
    let thought = added["thought_id"].as_str().expect("thought ID");
    let operation = operation_id();
    let copied = success(
        root,
        &[
            "thoughts",
            "send",
            "source",
            thought,
            "destination",
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(copied["source_removed"], false);
    assert_eq!(copied["destination_session_id"], destination);
    let destination_thought = copied["destination_thought_id"]
        .as_str()
        .expect("destination thought");
    let inspected = success(
        root,
        &["thoughts", "inspect", "destination", destination_thought],
        None,
    );
    assert_eq!(inspected["thought"]["content"], "exact\n内容");
    let replay = success(
        root,
        &[
            "thoughts",
            "send",
            "source",
            thought,
            "destination",
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(replay["destination_receipt"]["idempotent_replay"], true);

    let move_operation = operation_id();
    let remove_operation = operation_id();
    let removed = send_and_remove(root, thought, &move_operation, &remove_operation);
    assert_eq!(removed["source_removed"], true);
    let source_after = success(root, &["thoughts", "list", "source"], None);
    assert!(
        source_after["thoughts"]
            .as_array()
            .expect("thoughts")
            .is_empty()
    );
    let destination_after = success(root, &["thoughts", "list", "destination"], None);
    assert_eq!(
        destination_after["thoughts"]
            .as_array()
            .expect("thoughts")
            .len(),
        2
    );
    let removal_replay = send_and_remove(root, thought, &move_operation, &remove_operation);
    assert_eq!(
        removal_replay["destination_receipt"]["idempotent_replay"],
        true
    );
    assert_eq!(
        removal_replay["source_removal_receipt"]["idempotent_replay"],
        true
    );
}

fn send_and_remove(
    root: &Path,
    thought: &str,
    move_operation: &str,
    remove_operation: &str,
) -> Value {
    success(
        root,
        &[
            "thoughts",
            "send",
            "source",
            thought,
            "destination",
            "--remove",
            "--operation-id",
            move_operation,
            "--remove-operation-id",
            remove_operation,
        ],
        None,
    )
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
    assert_eq!(capabilities["active_session_control"], cfg!(unix));
    assert_eq!(capabilities["control_protocol"], 7);
    assert_eq!(capabilities["active_session_read_sync"], true);
    assert_eq!(capabilities["cross_session_transfer"], true);
    assert_eq!(capabilities["exact_thought_replacement"], true);
    assert_eq!(capabilities["replacement_sha256_precondition"], true);
    assert_eq!(capabilities["durable_thought_collapse"], true);
    assert_eq!(capabilities["max_thought_stdin_bytes"], 131_072);
    assert_eq!(capabilities["herdr_submission"], true);
    assert_eq!(capabilities["herdr_managed_pane_required"], true);
    assert_eq!(capabilities["explicit_update_check"], true);

    let human_capabilities = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("capabilities")
        .output()
        .expect("human capability discovery");
    assert!(human_capabilities.status.success());
    let human = String::from_utf8(human_capabilities.stdout).expect("UTF-8 capabilities");
    let expected = if cfg!(unix) {
        "Active control: available"
    } else {
        "Active control: unavailable on this platform"
    };
    assert!(human.contains(expected));

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
fn thought_standard_input_has_one_explicit_transport_safe_bound() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let session = create_session(temporary.path());
    let oversized = "x".repeat(131_073);
    let output = run(
        temporary.path(),
        &["thoughts", "add", &session],
        Some(&oversized),
    );
    assert!(!output.status.success());
    let failure: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(failure["error"]["code"], "invalid_input");
    assert!(
        failure["error"]["message"]
            .as_str()
            .expect("message")
            .contains("131072-byte")
    );
}
