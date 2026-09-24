//! Real-binary named session get-or-create and unconditional creation contracts.

use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Barrier},
    thread,
};

use serde_json::Value;

use super::{operation_id, run, success};

fn error_code(output: &std::process::Output) -> Value {
    let value: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(value["ok"], false);
    value["error"].clone()
}

fn session_count(root: &Path) -> usize {
    success(root, &["sessions", "list", "--all"], None)["total"]
        .as_u64()
        .and_then(|total| usize::try_from(total).ok())
        .expect("session total")
}

#[test]
fn ensure_creates_once_then_reuses_the_same_named_session() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = workspace.path().to_str().expect("UTF-8 workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical workspace");

    let created = success(
        root,
        &[
            "sessions",
            "ensure",
            "--name",
            "agent-os-claude",
            "--cwd",
            cwd,
        ],
        None,
    );
    let session = created["session_id"].as_str().expect("session ID");
    assert_eq!(created["disposition"], "created");
    assert_eq!(created["name"], "agent-os-claude");
    assert_eq!(
        created["origin_cwd"],
        canonical.to_str().expect("UTF-8 path")
    );
    assert_eq!(created["state"], "resumable");
    assert_eq!(created["resume_command"], format!("proqi -r {session}"));

    for _ in 0..3 {
        let reused = success(
            root,
            &[
                "sessions",
                "ensure",
                "--name",
                "agent-os-claude",
                "--cwd",
                cwd,
            ],
            None,
        );
        assert_eq!(reused["session_id"], session);
        assert_eq!(reused["disposition"], "reused");
    }
    let listed = success(root, &["sessions", "list"], None);
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["sessions"][0]["name"], "agent-os-claude");
}

#[test]
fn concurrent_ensure_processes_create_at_most_one_session() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = Arc::new(temporary.path().to_path_buf());
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = Arc::new(workspace.path().to_path_buf());
    let contenders = 20;
    let barrier = Arc::new(Barrier::new(contenders));
    let workers = (0..contenders)
        .map(|_| {
            let root = Arc::clone(&root);
            let cwd = Arc::clone(&cwd);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut command = Command::new(env!("CARGO_BIN_EXE_proqi"));
                command
                    .arg("--state-dir")
                    .arg(root.as_path())
                    .args(["--json", "sessions", "ensure", "--name", "shared", "--cwd"])
                    .arg(cwd.as_path())
                    .env_remove("HERDR_ENV")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                barrier.wait();
                command.output().expect("run ensure")
            })
        })
        .collect::<Vec<_>>();
    let outputs = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();

    let mut sessions = std::collections::BTreeSet::new();
    let mut created = 0;
    for output in &outputs {
        let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
        assert!(
            output.status.success(),
            "ensure failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        sessions.insert(value["data"]["session_id"].as_str().expect("ID").to_owned());
        if value["data"]["disposition"] == "created" {
            created += 1;
        }
    }
    assert_eq!(sessions.len(), 1, "every caller receives the same session");
    assert_eq!(created, 1);
    assert_eq!(session_count(&root), 1);
}

#[test]
fn ambiguous_and_conflicting_names_are_structured_errors_without_writes() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let first = tempfile::tempdir().expect("first workspace");
    let second = tempfile::tempdir().expect("second workspace");
    let first_cwd = first.path().to_str().expect("UTF-8");
    let second_cwd = second.path().to_str().expect("UTF-8");
    let a = success(
        root,
        &["sessions", "create", "--name", "dup", "--cwd", first_cwd],
        None,
    );
    let b = success(
        root,
        &["sessions", "create", "--name", "dup", "--cwd", first_cwd],
        None,
    );
    assert_ne!(a["session_id"], b["session_id"]);

    let ambiguous = run(
        root,
        &["sessions", "ensure", "--name", "dup", "--cwd", first_cwd],
        None,
    );
    assert_eq!(ambiguous.status.code(), Some(4));
    let error = error_code(&ambiguous);
    assert_eq!(error["code"], "ambiguous_session");
    assert_eq!(
        error["details"]["matches"]
            .as_array()
            .expect("matches")
            .len(),
        2
    );

    let conflict = run(
        root,
        &["sessions", "ensure", "--name", "dup", "--cwd", second_cwd],
        None,
    );
    assert_eq!(conflict.status.code(), Some(7));
    let error = error_code(&conflict);
    assert_eq!(error["code"], "session_name_conflict");
    assert_eq!(error["details"]["name"], "dup");
    let conflicting = error["details"]["sessions"].as_array().expect("sessions");
    assert_eq!(conflicting.len(), 2);
    assert!(conflicting.iter().all(|session| {
        session["origin_cwd"]
            == std::fs::canonicalize(first.path())
                .expect("canonical")
                .to_str()
                .expect("UTF-8")
    }));
    assert_eq!(session_count(root), 2);
}

#[test]
fn invalid_names_and_directories_fail_before_any_session_exists() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = workspace.path().to_str().expect("UTF-8");
    let file = workspace.path().join("file");
    std::fs::write(&file, b"x").expect("file");
    let cases: [&[&str]; 5] = [
        &["sessions", "ensure", "--name", "", "--cwd", cwd],
        &["sessions", "ensure", "--name", " \t ", "--cwd", cwd],
        &[
            "sessions",
            "ensure",
            "--name",
            "x",
            "--cwd",
            "/definitely/missing/proqi",
        ],
        &[
            "sessions",
            "ensure",
            "--name",
            "x",
            "--cwd",
            file.to_str().expect("UTF-8"),
        ],
        &["sessions", "create", "--name", "   "],
    ];
    for arguments in cases {
        let output = run(root, arguments, None);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert_eq!(
            error_code(&output)["code"],
            "invalid_input",
            "{arguments:?}"
        );
    }
    let missing = run(root, &["sessions", "ensure", "--name", "x"], None);
    assert_eq!(error_code(&missing)["code"], "invalid_arguments");
    assert_eq!(session_count(root), 0);
}

#[cfg(unix)]
#[test]
fn symlinked_directories_and_exact_unicode_names_identify_one_session() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let workspace = tempfile::tempdir().expect("workspace");
    let target = workspace.path().join("real");
    std::fs::create_dir(&target).expect("target");
    let link = workspace.path().join("link");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let name = "ünïcödé 名前 🙂";
    let first = success(
        root,
        &[
            "sessions",
            "ensure",
            "--name",
            name,
            "--cwd",
            target.to_str().expect("UTF-8"),
        ],
        None,
    );
    let through_link = success(
        root,
        &[
            "sessions",
            "ensure",
            "--name",
            name,
            "--cwd",
            link.to_str().expect("UTF-8"),
        ],
        None,
    );
    assert_eq!(through_link["session_id"], first["session_id"]);
    assert_eq!(through_link["disposition"], "reused");
    assert_eq!(through_link["name"], name);

    let distinct = success(
        root,
        &[
            "sessions",
            "ensure",
            "--name",
            &format!(" {name}"),
            "--cwd",
            target.to_str().expect("UTF-8"),
        ],
        None,
    );
    assert_ne!(distinct["session_id"], first["session_id"]);
}

#[test]
fn create_with_an_operation_identity_replays_and_rejects_divergent_reuse() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let workspace = tempfile::tempdir().expect("workspace");
    let cwd = workspace.path().to_str().expect("UTF-8");
    let operation = operation_id();
    let arguments = [
        "sessions",
        "create",
        "--name",
        "scratch",
        "--cwd",
        cwd,
        "--operation-id",
        &operation,
    ];
    let created = success(root, &arguments, None);
    assert_eq!(created["disposition"], "created");
    assert_eq!(created["receipt"]["operation_id"], operation);
    assert_eq!(created["receipt"]["idempotent_replay"], false);
    let replay = success(root, &arguments, None);
    assert_eq!(replay["session_id"], created["session_id"]);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);

    let divergent = run(
        root,
        &[
            "sessions",
            "create",
            "--name",
            "other",
            "--cwd",
            cwd,
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(error_code(&divergent)["code"], "idempotency_conflict");
    let session = created["session_id"].as_str().expect("ID");
    let reused_for_trash = run(
        root,
        &["sessions", "trash", session, "--operation-id", &operation],
        None,
    );
    assert_eq!(
        error_code(&reused_for_trash)["code"],
        "idempotency_conflict"
    );
    assert_eq!(session_count(root), 1);
}

#[test]
fn a_creation_retry_after_prune_reports_the_session_as_gone() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let operation = operation_id();
    let arguments = [
        "sessions",
        "create",
        "--name",
        "brief",
        "--operation-id",
        &operation,
    ];
    let created = success(root, &arguments, None);
    let session = created["session_id"].as_str().expect("ID").to_owned();
    success(root, &["sessions", "trash", &session], None);
    success(root, &["sessions", "prune", &session, "--yes"], None);

    let retry = run(root, &arguments, None);
    assert_eq!(retry.status.code(), Some(3));
    assert_eq!(error_code(&retry)["code"], "session_not_found");
    assert_eq!(
        session_count(root),
        0,
        "a retry never resurrects a pruned session"
    );
}

#[test]
fn human_named_creation_needs_no_terminal() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--state-dir")
        .arg(temporary.path())
        .args(["sessions", "create", "--name", "plain"])
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .expect("human create");
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).expect("UTF-8");
    assert!(human.contains("created"));
    assert!(human.contains("Resume later: proqi -r ses_"));
}

#[cfg(unix)]
#[test]
fn a_create_retry_may_switch_between_the_default_and_an_equivalent_cwd() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let workspace = tempfile::tempdir().expect("workspace");
    let target = workspace.path().join("real");
    std::fs::create_dir(&target).expect("target");
    let link = workspace.path().join("link");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let operation = operation_id();
    let created = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .current_dir(&link)
        .arg("--state-dir")
        .arg(root)
        .args(["--json", "sessions", "create", "--name", "cwd-forms"])
        .args(["--operation-id", &operation])
        .env_remove("HERDR_ENV")
        .stdin(Stdio::null())
        .output()
        .expect("create from the default directory");
    assert!(created.status.success());
    let created: Value = serde_json::from_slice(&created.stdout).expect("JSON");

    let replay = success(
        root,
        &[
            "sessions",
            "create",
            "--name",
            "cwd-forms",
            "--cwd",
            link.to_str().expect("UTF-8"),
            "--operation-id",
            &operation,
        ],
        None,
    );
    assert_eq!(replay["session_id"], created["data"]["session_id"]);
    assert_eq!(replay["receipt"]["idempotent_replay"], true);
    assert_eq!(session_count(root), 1);
}
