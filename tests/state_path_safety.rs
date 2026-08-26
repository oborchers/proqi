//! Real-process state-path containment regressions.

#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::{Command, Output, Stdio},
};

use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn run(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run Proqi")
}

fn assert_error(output: &Output, code: &str) {
    assert!(
        !output.status.success(),
        "unsafe startup succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(response["error"]["code"], code);
}

fn directory_entries(path: &Path) -> Vec<String> {
    let mut entries = fs::read_dir(path)
        .expect("read target")
        .map(|entry| {
            entry
                .expect("target entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn digest(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).expect("read digest input")).to_vec()
}

#[test]
fn data_symlink_is_rejected_by_doctor_and_startup_without_target_writes() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("state");
    let target = fixture.path().join("target");
    fs::create_dir(&root).expect("state root");
    fs::create_dir(&target).expect("target");
    symlink(&target, root.join("data")).expect("data symlink");

    let doctor = run(&root, &["doctor"]);
    assert_error(&doctor, "doctor_failed");
    assert!(directory_entries(&target).is_empty());

    let startup = run(&root, &[]);
    assert_error(&startup, "unsafe_state_path");
    assert!(directory_entries(&target).is_empty());
}

#[test]
fn linked_state_root_and_other_owned_leaves_fail_before_target_writes() {
    for leaf in ["config", "cache", "runtime"] {
        let fixture = tempfile::tempdir().expect("fixture");
        let root = fixture.path().join("state");
        let target = fixture.path().join("target");
        fs::create_dir(&root).expect("state root");
        fs::create_dir(&target).expect("target");
        symlink(&target, root.join(leaf)).expect("leaf symlink");
        assert_error(&run(&root, &[]), "unsafe_state_path");
        assert!(directory_entries(&target).is_empty());
    }

    let fixture = tempfile::tempdir().expect("root fixture");
    let target = fixture.path().join("target");
    let linked = fixture.path().join("state");
    fs::create_dir(&target).expect("target");
    symlink(&target, &linked).expect("state symlink");
    assert_error(&run(&linked, &[]), "unsafe_state_path");
    assert!(directory_entries(&target).is_empty());
}

#[test]
fn linked_database_preserves_an_unrelated_sqlite_fixture_byte_for_byte() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("state");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("data directory");
    let target = fixture.path().join("unrelated.sqlite3");
    let connection = Connection::open(&target).expect("unrelated database");
    connection
        .execute_batch("CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES ('keep');")
        .expect("sentinel database");
    drop(connection);
    let before = digest(&target);
    symlink(&target, data.join("proqi.sqlite3")).expect("database symlink");

    assert_error(&run(&root, &[]), "storage_failed");
    assert_eq!(digest(&target), before);
    let connection = Connection::open(&target).expect("verify unrelated database");
    let value: String = connection
        .query_row("SELECT value FROM sentinel", [], |row| row.get(0))
        .expect("sentinel row");
    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |row| row.get(0),
        )
        .expect("table count");
    assert_eq!(value, "keep");
    assert_eq!(tables, 1);
}

#[test]
fn linked_backup_directory_is_rejected_without_external_files() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().join("state");
    let data = root.join("data");
    let target = fixture.path().join("target");
    fs::create_dir_all(&data).expect("data directory");
    fs::create_dir(&target).expect("target");
    Connection::open(data.join("proqi.sqlite3"))
        .expect("legacy database")
        .execute("CREATE TABLE legacy(value TEXT)", [])
        .expect("legacy table");
    symlink(&target, data.join("backups")).expect("backup symlink");

    assert_error(&run(&root, &[]), "storage_failed");
    assert!(directory_entries(&target).is_empty());
}
