//! Read-only doctor and diagnostic independence contracts.

use super::*;

#[test]
fn doctor_reports_fresh_state_without_initializing_it() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("state");
    std::fs::create_dir(&root).expect("state root");
    let report = success(&root, &["doctor"], None);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["overall_status"], "skipped");
    assert!(
        report["checks"]
            .as_array()
            .is_some_and(|checks| checks.iter().any(|check| check["id"] == "sqlite"))
    );
    assert_eq!(
        std::fs::read_dir(&root).expect("read state root").count(),
        0
    );
}

#[test]
fn diagnostics_collection_does_not_open_or_migrate_sqlite() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let data = root.join("data");
    std::fs::create_dir(&data).expect("data directory");
    let database = data.join("proqi.sqlite3");
    let malformed = b"not a sqlite database";
    std::fs::write(&database, malformed).expect("malformed database");
    let output = root.join("support.json");
    let collected = success(
        root,
        &[
            "diagnostics",
            "collect",
            "--output",
            output.to_str().expect("UTF-8 output"),
        ],
        None,
    );
    assert_eq!(collected["bundle_schema_version"], 1);
    assert_eq!(std::fs::read(&database).expect("database bytes"), malformed);
    assert!(!root.join("runtime").exists());
    assert!(!root.join("config").exists());
    assert!(!root.join("cache").exists());
}

#[cfg(unix)]
#[test]
fn doctor_fails_with_stable_json_for_unsafe_configuration() {
    use std::os::unix::fs::symlink;
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let config = root.join("config");
    std::fs::create_dir(&config).expect("config directory");
    let target = root.join("elsewhere.toml");
    std::fs::write(&target, "theme = 'dark'\n").expect("config target");
    symlink(&target, config.join("config.toml")).expect("config symlink");
    let failed = run(root, &["doctor"], None);
    assert!(!failed.status.success());
    let response: Value = serde_json::from_slice(&failed.stdout).expect("error JSON");
    assert_eq!(response["error"]["code"], "doctor_failed");
    assert_eq!(response["error"]["details"]["overall_status"], "fail");
}
