//! Real-binary exact split, extract, merge, duplicate, and reflow contracts.

use std::str::FromStr as _;

use proqi::domain::{ContentAnnotation, ContentAnnotationKind, ThoughtId};
use rusqlite::{Connection, params};
use serde_json::Value;

use super::{add_with_idempotency, create_session, operation_id, run, success};

fn inspect(root: &std::path::Path, session: &str, thought: &str) -> Value {
    success(root, &["thoughts", "inspect", session, thought], None)["thought"].clone()
}

#[test]
fn unicode_split_and_control_extract_are_exact_named_and_reversible() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let source = add_with_idempotency(root, &session, "Grüße 👩‍💻\nnext");
    success(
        root,
        &["thoughts", "rename", &session, &source, "Source"],
        None,
    );
    let before = inspect(root, &session, &source);
    let digest = before["content_sha256"].as_str().expect("digest");
    let invalid_boundary = run(
        root,
        &[
            "thoughts",
            "split",
            &session,
            &source,
            "3",
            "--expected-sha256",
            digest,
        ],
        None,
    );
    assert!(!invalid_boundary.status.success());
    let invalid_boundary: Value =
        serde_json::from_slice(&invalid_boundary.stdout).expect("boundary JSON");
    assert_eq!(invalid_boundary["error"]["code"], "invalid_state");
    assert_eq!(
        inspect(root, &session, &source)["content"],
        before["content"]
    );
    let split = success(
        root,
        &[
            "thoughts",
            "split",
            &session,
            &source,
            "8",
            "--expected-sha256",
            digest,
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    let split_ids = split["item_ids"].as_array().expect("split IDs");
    assert_eq!(split_ids[0]["kind"], "thought");
    assert_eq!(split_ids[0]["id"], source);
    let neighbor = split_ids[1]["id"].as_str().expect("neighbor ID");
    assert_eq!(inspect(root, &session, &source)["content"], "Grüße ");
    assert_eq!(inspect(root, &session, &source)["name"], "Source");
    assert_eq!(inspect(root, &session, neighbor)["content"], "👩‍💻\nnext");
    assert!(inspect(root, &session, neighbor)["name"].is_null());

    success(root, &["thoughts", "undo", &session], None);
    assert_eq!(
        inspect(root, &session, &source)["content"],
        "Grüße 👩‍💻\nnext"
    );
    let after_undo = success(root, &["thoughts", "list", &session], None);
    assert!(
        !after_undo["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == neighbor)
    );
    success(root, &["thoughts", "redo", &session], None);
    assert_eq!(inspect(root, &session, neighbor)["content"], "👩‍💻\nnext");

    assert_control_extract(root, &session);
}

fn assert_control_extract(root: &std::path::Path, session: &str) {
    let controls = add_with_idempotency(root, session, "A\tB\u{7}C");
    let control_snapshot = inspect(root, session, &controls);
    let control_digest = control_snapshot["content_sha256"].as_str().expect("digest");
    let extracted = success(
        root,
        &[
            "thoughts",
            "extract",
            session,
            &controls,
            "1",
            "3",
            "--expected-sha256",
            control_digest,
        ],
        None,
    );
    let extracted_id = extracted["item_ids"][1]["id"]
        .as_str()
        .expect("extracted ID");
    assert_eq!(inspect(root, session, &controls)["content"], "A\u{7}C");
    assert_eq!(inspect(root, session, extracted_id)["content"], "\tB");

    let remaining = inspect(root, session, &controls);
    let remaining_digest = remaining["content_sha256"].as_str().expect("digest");
    let unsupported = run(
        root,
        &[
            "thoughts",
            "reflow",
            session,
            &controls,
            "--expected-sha256",
            remaining_digest,
        ],
        None,
    );
    assert!(!unsupported.status.success());
    let unsupported: Value =
        serde_json::from_slice(&unsupported.stdout).expect("unsupported-control JSON");
    assert_eq!(unsupported["error"]["code"], "no_change");
    assert_eq!(inspect(root, session, &controls)["content"], "A\u{7}C");
}

#[test]
fn transformations_reject_stale_boundaries_and_separator_blocked_merge() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let first = add_with_idempotency(root, &session, "first");
    let second = add_with_idempotency(root, &session, "second");
    let first_state = inspect(root, &session, &first);
    let first_digest = first_state["content_sha256"]
        .as_str()
        .expect("first digest");
    let second_state = inspect(root, &session, &second);
    let second_digest = second_state["content_sha256"]
        .as_str()
        .expect("second digest");

    let invalid_boundary = run(
        root,
        &[
            "thoughts",
            "split",
            &session,
            &first,
            "99",
            "--expected-sha256",
            first_digest,
        ],
        None,
    );
    assert!(!invalid_boundary.status.success());
    let invalid_boundary: Value =
        serde_json::from_slice(&invalid_boundary.stdout).expect("boundary JSON");
    assert_eq!(invalid_boundary["error"]["code"], "invalid_state");

    let stale = run(
        root,
        &[
            "thoughts",
            "reflow",
            &session,
            &first,
            "--expected-sha256",
            &"00".repeat(32),
        ],
        None,
    );
    assert!(!stale.status.success());
    let stale: Value = serde_json::from_slice(&stale.stdout).expect("stale JSON");
    assert_eq!(stale["error"]["code"], "content_conflict");

    success(
        root,
        &["items", "insert-separator", &session, "--position", "1"],
        None,
    );
    let blocked = run(
        root,
        &[
            "thoughts",
            "merge",
            &session,
            &first,
            &second,
            "--expected-sha256",
            first_digest,
            "--expected-sha256",
            second_digest,
        ],
        None,
    );
    assert!(!blocked.status.success());
    let blocked: Value = serde_json::from_slice(&blocked.stdout).expect("blocked JSON");
    assert_eq!(blocked["ok"], false);
    assert_eq!(blocked["error"]["code"], "invalid_state");
    assert_eq!(inspect(root, &session, &first)["content"], "first");
    assert_eq!(inspect(root, &session, &second)["content"], "second");
}

#[test]
fn merge_preserves_names_history_and_idempotency() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    std::fs::write(
        root.join("config/config.toml"),
        "merge_separator = \"\\n---\\n\"\n",
    )
    .expect("configured merge separator");
    let first = add_with_idempotency(root, &session, "eins");
    let second = add_with_idempotency(root, &session, "二");
    success(
        root,
        &["thoughts", "rename", &session, &first, "Keeper"],
        None,
    );
    success(
        root,
        &["thoughts", "rename", &session, &second, "Removed"],
        None,
    );
    let first_digest = inspect(root, &session, &first)["content_sha256"]
        .as_str()
        .expect("digest")
        .to_owned();
    let second_digest = inspect(root, &session, &second)["content_sha256"]
        .as_str()
        .expect("digest")
        .to_owned();
    let operation = operation_id();
    let merge_arguments = [
        "thoughts",
        "merge",
        &session,
        &first,
        &second,
        "--expected-sha256",
        &first_digest,
        "--expected-sha256",
        &second_digest,
        "--operation-id",
        &operation,
    ];
    let merged = success(root, &merge_arguments, None);
    assert_eq!(merged["receipt"]["idempotent_replay"], false);
    let replay = success(root, &merge_arguments, None);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    let merged_first = inspect(root, &session, &first);
    assert_eq!(merged_first["content"], "eins\n---\n二");
    assert_eq!(merged_first["name"], "Keeper");
    success(root, &["thoughts", "undo", &session], None);
    assert_eq!(inspect(root, &session, &second)["name"], "Removed");
}

#[test]
fn large_reflow_is_exact_idempotent_and_reports_no_change() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let large_body = "  Narrow\t source wrapping  \n".repeat(1_024);
    let large = add_with_idempotency(root, &session, &large_body);
    success(
        root,
        &["thoughts", "rename", &session, &large, "Large source"],
        None,
    );
    let large_digest = inspect(root, &session, &large)["content_sha256"]
        .as_str()
        .expect("digest")
        .to_owned();
    success(
        root,
        &[
            "thoughts",
            "reflow",
            &session,
            &large,
            "--expected-sha256",
            &large_digest,
        ],
        None,
    );
    let cleaned = inspect(root, &session, &large)["content"]
        .as_str()
        .expect("cleaned content")
        .to_owned();
    assert_eq!(inspect(root, &session, &large)["name"], "Large source");
    assert!(!cleaned.contains('\t'));
    assert!(!cleaned.contains("  "));
    let clean_digest = inspect(root, &session, &large)["content_sha256"]
        .as_str()
        .expect("clean digest")
        .to_owned();
    let no_change = run(
        root,
        &[
            "thoughts",
            "reflow",
            &session,
            &large,
            "--expected-sha256",
            &clean_digest,
        ],
        None,
    );
    assert!(!no_change.status.success());
    let no_change: Value = serde_json::from_slice(&no_change.stdout).expect("no-change JSON");
    assert_eq!(no_change["error"]["code"], "no_change");
}

#[test]
fn duplicate_and_split_preserve_names_annotations_and_fresh_ordinals() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let content = "/tmp/asset.png tail";
    let source = add_with_idempotency(root, &session, content);
    success(
        root,
        &["thoughts", "rename", &session, &source, "Attachment source"],
        None,
    );
    install_attachment(root, &source, content);

    let duplicated = success(
        root,
        &[
            "items",
            "duplicate",
            &session,
            &source,
            "--operation-id",
            &operation_id(),
        ],
        None,
    );
    let duplicate = duplicated["item_ids"][0]["id"]
        .as_str()
        .expect("duplicate ID");
    assert_eq!(
        inspect(root, &session, duplicate)["name"],
        "Attachment source"
    );
    assert_attachment(root, &source, 1);
    assert_attachment(root, duplicate, 2);

    let duplicate_state = inspect(root, &session, duplicate);
    let digest = duplicate_state["content_sha256"].as_str().expect("digest");
    let split = success(
        root,
        &[
            "thoughts",
            "split",
            &session,
            duplicate,
            "0",
            "--expected-sha256",
            digest,
        ],
        None,
    );
    let right = split["item_ids"][1]["id"].as_str().expect("right ID");
    assert_eq!(
        inspect(root, &session, duplicate)["name"],
        "Attachment source"
    );
    assert!(inspect(root, &session, right)["name"].is_null());
    assert!(load_annotations(root, duplicate).is_empty());
    assert_attachment(root, right, 2);
}

fn install_attachment(root: &std::path::Path, thought: &str, content: &str) {
    let path = "/tmp/asset.png";
    let annotations = vec![ContentAnnotation {
        start: 0,
        end: path.len(),
        kind: ContentAnnotationKind::Attachment {
            ordinal: Some(1_u64.try_into().expect("fixture ordinal")),
            image: true,
            display_name: "asset.png".to_owned(),
        },
    }];
    proqi::domain::validate_annotations(content, &annotations).expect("valid fixture annotation");
    let thought = ThoughtId::from_str(thought).expect("thought ID");
    let connection =
        Connection::open(root.join("data/proqi.sqlite3")).expect("open fixture database");
    connection
        .execute(
            "UPDATE thoughts SET annotations_json = ?1 WHERE id = ?2",
            params![
                serde_json::to_string(&annotations).expect("annotation JSON"),
                thought.database_bytes().as_slice()
            ],
        )
        .expect("install fixture annotation");
    connection
        .execute(
            "UPDATE sessions SET attachment_image_high = 1
             WHERE id = (SELECT session_id FROM thoughts WHERE id = ?1)",
            [thought.database_bytes().as_slice()],
        )
        .expect("install fixture ordinal high-water mark");
}

fn assert_attachment(root: &std::path::Path, thought: &str, ordinal: u64) {
    let annotations = load_annotations(root, thought);
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].start, 0);
    assert_eq!(annotations[0].end, "/tmp/asset.png".len());
    assert!(matches!(
        &annotations[0].kind,
        ContentAnnotationKind::Attachment {
            ordinal: Some(actual),
            image: true,
            display_name,
        } if actual.get() == ordinal && display_name == "asset.png"
    ));
}

fn load_annotations(root: &std::path::Path, thought: &str) -> Vec<ContentAnnotation> {
    let thought = ThoughtId::from_str(thought).expect("thought ID");
    let encoded: String = Connection::open(root.join("data/proqi.sqlite3"))
        .expect("open fixture database")
        .query_row(
            "SELECT annotations_json FROM thoughts WHERE id = ?1",
            [thought.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("read durable annotations");
    serde_json::from_str(&encoded).expect("annotation JSON")
}
