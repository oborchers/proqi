//! Reports retain counters and typed fixture identity, never arbitrary strings.

use super::{Evidence, persist};
use serde_json::{Value, json};
use std::fs;

#[test]
fn failure_report_roundtrip_excludes_payload_paths_and_unknown_diagnostic_fields() {
    let state = tempfile::tempdir().expect("synthetic state");
    let artifacts = tempfile::tempdir().expect("local report sink");
    fs::create_dir_all(state.path().join("data/diagnostics")).expect("diagnostic directory");
    fs::write(
        state.path().join("data/diagnostics/events"),
        "{\"stage\":\"migration_started\",\"private_payload\":\"DO_NOT_EXPORT\"}\n",
    )
    .expect("synthetic diagnostic");
    let evidence = Evidence::new(state.path(), 15);
    evidence.stage("coordination_assertions");
    evidence.execution(&json!({"restart_requests": 15, "restart_accepted": 14,
        "status": {"status": "aborted", "code": "DO_NOT_EXPORT"},
        "private_payload": "DO_NOT_EXPORT", "resumable_sessions": ["DO_NOT_EXPORT"]}));
    fs::write(state.path().join("gateway-trace.jsonl"), "DO_NOT_EXPORT\n")
        .expect("malformed trace");
    let summary = evidence.summary();
    let path = persist(artifacts.path(), &summary).expect("retain bounded report");
    let bytes = fs::read(path).expect("report bytes");
    assert!(bytes.len() < 1024 * 1024);
    let report: Value = serde_json::from_slice(&bytes).expect("report JSON");
    assert_eq!(report["execution"]["restart_accepted"], 14);
    assert_eq!(report["execution"]["restart_requests"], 15);
    assert_eq!(report["diagnostic_counts"]["migration_started"], 1);
    assert_eq!(report["gateway_events"], json!([]));
    assert!(
        !String::from_utf8(bytes)
            .expect("report UTF-8")
            .contains("DO_NOT_EXPORT")
    );
    assert!(
        !report
            .to_string()
            .contains(&state.path().to_string_lossy().to_string())
    );
}
