//! Session creation, compatibility refusal, identifiers, and active-owner CLI contracts.

use proqi::{
    adapters::runtime::{FileRuntimeCoordinator, SystemIdGenerator},
    domain::{SessionId, Timestamp},
    ports::{environment::IdGenerator, runtime::RuntimeCoordinator},
};
use serde_json::Value;

use super::{create_session, run, success};

#[test]
fn json_fresh_launch_is_empty_and_retains_interactive_onboarding_eligibility() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let thoughts = success(root, &["thoughts", "list", &session], None);
    assert!(thoughts["thoughts"].as_array().is_some_and(Vec::is_empty));
    let connection =
        rusqlite::Connection::open(root.join("data/proqi.sqlite3")).expect("open database");
    let completed: i64 = connection
        .query_row(
            "SELECT completed_version FROM onboarding_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .expect("onboarding marker");
    assert_eq!(completed, 0);
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
fn session_administration_has_restart_safe_cli_undo_and_redo() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    success(root, &["sessions", "rename", &session, "research"], None);
    success(root, &["sessions", "trash", "research"], None);

    let undo_trash = success(root, &["sessions", "undo"], None);
    assert_eq!(undo_trash["operation"], "session trash");
    let restored = success(root, &["sessions", "list", "--query", "research"], None);
    assert_eq!(restored["sessions"][0]["id"], session);

    let undo_rename = success(root, &["sessions", "undo"], None);
    assert_eq!(undo_rename["operation"], "session rename");
    let absent = success(root, &["sessions", "list", "--query", "research"], None);
    assert!(absent["sessions"].as_array().is_some_and(Vec::is_empty));

    let redo_rename = success(root, &["sessions", "redo"], None);
    assert_eq!(redo_rename["operation"], "session rename");
    let renamed = success(root, &["sessions", "list", "--query", "research"], None);
    assert_eq!(renamed["sessions"][0]["id"], session);

    success(root, &["sessions", "rename", &session, "divergent"], None);
    let unavailable = run(root, &["sessions", "redo"], None);
    assert!(!unavailable.status.success());
    let error: Value = serde_json::from_slice(&unavailable.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "history_unavailable");
}

#[test]
fn inactive_noop_session_rename_reports_the_same_success_as_an_active_owner() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let renamed = success(root, &["sessions", "rename", &session, "stable"], None);
    assert_eq!(renamed["status"], "renamed");
    let unchanged = success(root, &["sessions", "rename", &session, "stable"], None);
    assert_eq!(unchanged["status"], "renamed");
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
