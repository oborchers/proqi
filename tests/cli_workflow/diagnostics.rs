//! Structured local diagnostics contracts.

use super::*;

fn diagnostic_logs(root: &Path) -> Vec<std::path::PathBuf> {
    let directory = root.join("data/diagnostics");
    let mut paths = std::fs::read_dir(directory)
        .expect("diagnostics directory")
        .map(|entry| entry.expect("diagnostic entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".jsonl"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[test]
fn file_diagnostics_are_private_bounded_and_content_redacted() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let session = create_session(temporary.path());
    let secret = "never-log-this-thought-body";
    let _thought = add_with_idempotency(temporary.path(), &session, secret);
    let logs = diagnostic_logs(temporary.path());
    assert!(!logs.is_empty());
    let content = logs
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("diagnostic log"))
        .collect::<String>();
    assert!(content.contains("diagnostics_initialized"));
    assert!(content.contains("schema_lifecycle"));
    assert!(content.contains("\"stage\":\"ready\""));
    assert!(content.contains("command_succeeded"));
    assert!(!content.contains(secret));
    for line in content.lines() {
        let event: Value = serde_json::from_str(line).expect("structured event");
        assert!(event["event"].is_string());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        for path in logs {
            let metadata = std::fs::metadata(path).expect("log metadata");
            assert!(metadata.len() <= 1024 * 1024);
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }
    }
}

#[test]
fn diagnostics_collection_is_versioned_private_redacted_and_never_overwrites() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let secret = "private-thought-content-must-not-leave-sqlite";
    let session = create_session(temporary.path());
    let _thought = add_with_idempotency(temporary.path(), &session, secret);
    let output = temporary.path().join("support.json");
    let output_text = output.to_str().expect("UTF-8 output path");
    let result = success(
        temporary.path(),
        &["diagnostics", "collect", "--output", output_text],
        None,
    );
    assert_eq!(result["bundle_schema_version"], 1);
    assert!(result["files"].as_u64().is_some_and(|files| files > 0));

    let content = std::fs::read_to_string(&output).expect("support bundle");
    assert!(!content.contains(secret));
    assert!(!content.contains(temporary.path().to_string_lossy().as_ref()));
    let bundle: Value = serde_json::from_str(&content).expect("bundle JSON");
    assert_eq!(bundle["schema_version"], 1);
    assert!(
        bundle["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty())
    );

    let repeated = run(
        temporary.path(),
        &["diagnostics", "collect", "--output", output_text],
        None,
    );
    assert!(!repeated.status.success());
    let error: Value = serde_json::from_slice(&repeated.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "diagnostics_failed");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        assert_eq!(
            std::fs::metadata(output)
                .expect("bundle metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
