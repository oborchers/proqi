//! Public CLI and JSON contracts for optional thought names.

use super::*;

#[test]
fn rename_clear_validation_and_retry_keep_name_outside_the_human_body() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let body = "Grüße 👩‍💻\n第二行\n";
    let added = success(root, &["thoughts", "add", &session], Some(body));
    let thought = added["thought_id"].as_str().expect("thought ID");

    assert_basic_rename(root, &session, thought, body);
    assert_retry_safe_no_op(root, &session, thought);
    assert_validation_and_clear(root, &session, thought);
    assert_human_inspect_is_body(root, &session, thought, body);
}

fn assert_basic_rename(root: &Path, session: &str, thought: &str, body: &str) {
    let rename_operation = operation_id();
    let renamed = success(
        root,
        &[
            "thoughts",
            "rename",
            session,
            thought,
            "Release 計画",
            "--operation-id",
            &rename_operation,
        ],
        None,
    );
    assert_eq!(renamed["receipt"]["idempotent_replay"], false);
    let named = success(root, &["thoughts", "inspect", session, thought], None);
    assert_eq!(named["thought"]["name"], "Release 計画");
    assert_eq!(named["thought"]["content"], body);

    success(
        root,
        &["thoughts", "rename", session, thought, "  Trimmed title  "],
        None,
    );
    let trimmed = success(root, &["thoughts", "inspect", session, thought], None);
    assert_eq!(trimmed["thought"]["name"], "Trimmed title");
    success(root, &["thoughts", "rename", session, thought, "   "], None);
    let blank = success(root, &["thoughts", "inspect", session, thought], None);
    assert!(blank["thought"]["name"].is_null());
    success(
        root,
        &["thoughts", "rename", session, thought, "Release 計画"],
        None,
    );
}

fn assert_retry_safe_no_op(root: &Path, session: &str, thought: &str) {
    let no_op = operation_id();
    let arguments = [
        "thoughts",
        "rename",
        session,
        thought,
        "Release 計画",
        "--operation-id",
        &no_op,
    ];
    let unchanged = success(root, &arguments, None);
    assert_eq!(unchanged["receipt"]["idempotent_replay"], false);
    let replay = success(root, &arguments, None);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    let conflict = run(
        root,
        &[
            "thoughts",
            "rename",
            session,
            thought,
            "Different",
            "--operation-id",
            &no_op,
        ],
        None,
    );
    assert!(!conflict.status.success());
    let conflict: Value = serde_json::from_slice(&conflict.stdout).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
}

fn assert_validation_and_clear(root: &Path, session: &str, thought: &str) {
    let invalid = run(
        root,
        &["thoughts", "rename", session, thought, "line\nbreak"],
        None,
    );
    assert!(!invalid.status.success());
    let invalid: Value = serde_json::from_slice(&invalid.stdout).expect("invalid-name JSON");
    assert_eq!(invalid["error"]["code"], "invalid_input");
    let long = "n".repeat(81);
    for name in ["tab\tname", long.as_str()] {
        let rejected = run(root, &["thoughts", "rename", session, thought, name], None);
        assert_eq!(rejected.status.code(), Some(2), "{name:?}");
        let rejected: Value = serde_json::from_slice(&rejected.stdout).expect("invalid-name JSON");
        assert_eq!(rejected["error"]["code"], "invalid_input", "{name:?}");
    }

    success(
        root,
        &["thoughts", "rename", session, thought, "--clear"],
        None,
    );
    let cleared = success(root, &["thoughts", "inspect", session, thought], None);
    assert!(cleared["thought"]["name"].is_null());
    success(
        root,
        &["thoughts", "rename", session, thought, "Release 計画"],
        None,
    );
}

fn assert_human_inspect_is_body(root: &Path, session: &str, thought: &str, body: &str) {
    let human = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(root)
        .args(["thoughts", "inspect", session, thought])
        .env_remove("HERDR_ENV")
        .output()
        .expect("human inspect");
    assert!(human.status.success());
    assert_eq!(human.stdout, format!("{body}\n").as_bytes());
}
