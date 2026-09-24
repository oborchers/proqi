//! Real-binary bounded pages for thought and session lists.

use std::path::Path;

use serde_json::Value;

use super::{create_session, run, success};

fn error(root: &Path, arguments: &[&str]) -> (Option<i32>, Value) {
    let output = run(root, arguments, None);
    let value: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    (output.status.code(), value["error"].clone())
}

fn page(root: &Path, arguments: &[&str]) -> (Vec<String>, Value, Value) {
    let data = success(root, arguments, None);
    let collection = if data.get("items").is_some() {
        "items"
    } else {
        "sessions"
    };
    let ids = data[collection]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["id"].as_str().expect("ID").to_owned())
        .collect();
    (ids, data["total"].clone(), data["next_after"].clone())
}

#[test]
fn thought_pages_walk_mixed_items_without_the_legacy_projection() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    for body in ["one", "two", "three"] {
        success(root, &["thoughts", "add", &session], Some(body));
    }
    success(
        root,
        &["items", "insert-separator", &session, "--position", "1"],
        None,
    );
    let complete = success(root, &["thoughts", "list", &session], None);
    assert!(complete.get("thoughts").is_none());
    assert_eq!(complete["total"], 4);
    assert!(complete["next_after"].is_null());
    let all = complete["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["id"].as_str().expect("ID").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(complete["items"][1]["kind"], "separator");
    assert_eq!(complete["items"][0]["content"], "one");
    assert!(complete["items"][0]["content_sha256"].is_string());

    let mut walked = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let mut arguments = vec!["thoughts", "list", session.as_str(), "--limit", "1"];
        if let Some(anchor) = after.as_deref() {
            arguments.extend(["--after", anchor]);
        }
        let (ids, total, next) = page(root, &arguments);
        assert_eq!(total, 4);
        assert_eq!(ids.len(), 1);
        walked.extend(ids);
        match next.as_str() {
            Some(anchor) => after = Some(anchor.to_owned()),
            None => break,
        }
    }
    assert_eq!(walked, all);

    let (large, _, next) = page(
        root,
        &["thoughts", "list", &session, "--limit", "4294967295"],
    );
    assert_eq!(large, all);
    assert!(next.is_null());
    let (after_last, _, next) = page(root, &["thoughts", "list", &session, "--after", &all[3]]);
    assert!(after_last.is_empty());
    assert!(next.is_null());
}

#[test]
fn invalid_limits_and_stale_anchors_fail_without_restarting() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let added = success(root, &["thoughts", "add", &session], Some("gone"));
    let thought = added["thought_id"].as_str().expect("thought ID").to_owned();
    success(root, &["thoughts", "delete", &session, &thought], None);

    for limit in ["0", "-1", "x", "4294967296"] {
        let (exit, failure) = error(root, &["thoughts", "list", &session, "--limit", limit]);
        assert_eq!(exit, Some(2), "{limit}");
        assert_eq!(failure["code"], "invalid_arguments", "{limit}");
    }
    let (exit, stale) = error(root, &["thoughts", "list", &session, "--after", &thought]);
    assert_eq!(exit, Some(3));
    assert_eq!(stale["code"], "cursor_not_found");
    let (exit, malformed) = error(root, &["thoughts", "list", &session, "--after", "ses_bad"]);
    assert_eq!(exit, Some(2));
    assert_eq!(malformed["code"], "invalid_identifier");
    let (_, wrong_kind) = error(root, &["sessions", "list", "--after", &thought]);
    assert_eq!(wrong_kind["code"], "invalid_identifier");
}

#[test]
fn session_pages_follow_ranking_and_filters() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let sessions = (0..3).map(|_| create_session(root)).collect::<Vec<_>>();
    success(root, &["sessions", "trash", &sessions[0]], None);

    let (live, total, _) = page(root, &["sessions", "list"]);
    assert_eq!(total, 2);
    let (first, _, next) = page(root, &["sessions", "list", "--limit", "1"]);
    assert_eq!(first, live[..1]);
    let anchor = next.as_str().expect("next page").to_owned();
    let (second, _, next) = page(
        root,
        &["sessions", "list", "--limit", "1", "--after", &anchor],
    );
    assert_eq!(second, live[1..]);
    assert!(next.is_null());

    let (all, total, _) = page(root, &["sessions", "list", "--all", "--limit", "10"]);
    assert_eq!(total, 3);
    assert_eq!(all.len(), 3);
    let (exit, trashed_anchor) = error(root, &["sessions", "list", "--after", &sessions[0]]);
    assert_eq!(exit, Some(3));
    assert_eq!(trashed_anchor["code"], "cursor_not_found");
}
