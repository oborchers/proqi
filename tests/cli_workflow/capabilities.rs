//! Real-binary launch behavior and installed capability manifest contracts.

use std::process::Command;

use super::success;

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
    assert_capability_manifest(root);

    let human_capabilities = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("capabilities")
        .output()
        .expect("human capability discovery");
    assert!(human_capabilities.status.success());
    let human = String::from_utf8(human_capabilities.stdout).expect("UTF-8 capabilities");
    assert!(human.contains("Sessions, board items, and thoughts are available"));
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

fn assert_capability_manifest(root: &std::path::Path) {
    let capabilities = success(root, &["capabilities"], None);
    assert_eq!(capabilities["cli_schema_version"], 1);
    assert_eq!(capabilities["active_session_control"], cfg!(unix));
    assert_eq!(capabilities["control_protocol"], 12);
    assert_eq!(capabilities["active_session_read_sync"], cfg!(unix));
    assert_eq!(capabilities["cross_session_transfer"], true);
    assert_eq!(capabilities["exact_thought_replacement"], true);
    assert_eq!(capabilities["replacement_sha256_precondition"], true);
    assert_eq!(capabilities["durable_thought_collapse"], true);
    assert_eq!(capabilities["durable_visual_separators"], true);
    assert_eq!(capabilities["semantic_separator_mutations"], true);
    assert_eq!(capabilities["mixed_board_item_mutations"], true);
    assert_eq!(capabilities["exact_text_transformations"], true);
    assert_eq!(
        capabilities["commands"],
        serde_json::json!([
            "capabilities",
            "completions",
            "update",
            "diagnostics",
            "doctor",
            "sessions",
            "items",
            "thoughts",
            "herdr"
        ])
    );
    assert_eq!(
        capabilities["operations"]["diagnostics"],
        serde_json::json!(["collect", "keypress"])
    );
    assert_eq!(
        capabilities["operations"]["items"],
        serde_json::json!(["insert-separator", "move", "delete", "duplicate"])
    );
    assert_eq!(capabilities["durable_browser_history"], true);
    assert_eq!(
        capabilities["operations"]["update"],
        serde_json::json!(["check"])
    );
    assert_eq!(capabilities["max_thought_stdin_bytes"], 131_072);
    assert_eq!(capabilities["herdr_submission"], true);
    assert_eq!(capabilities["herdr_managed_pane_required"], true);
    assert_eq!(capabilities["herdr_companion_toggle"], true);
    assert_eq!(
        capabilities["operations"]["herdr"],
        serde_json::json!(["toggle"])
    );
    assert_eq!(capabilities["explicit_update_check"], true);
}
