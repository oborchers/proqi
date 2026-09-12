use super::{ChangeClass, Classification, LocalPlan, classification_json, classify, local};

use std::{fs, path::Path, process::Command};

fn paths(values: &[&str]) -> Classification {
    classify(values.iter().copied())
}

#[test]
fn ordinary_markdown_uses_the_local_documentation_plan() {
    let result = paths(&["README.md", "docs/usage.md"]);
    assert!(result.docs_only);
    assert!(!result.coverage);
    assert!(!result.full_msrv);
    assert_eq!(result.local_plan, LocalPlan::Documentation);
}

#[test]
fn policy_and_classifier_changes_fail_closed() {
    for path in [
        ".github/workflows/ci.yml",
        "AGENTS.md",
        "context/ARCHITECTURE.md",
        "xtask/src/ci_changes.rs",
        "xtask/src/dev_gates.rs",
    ] {
        let result = paths(&[path]);
        assert_eq!(result.local_plan, LocalPlan::Full, "{path}");
        assert!(result.full_msrv, "{path}");
        assert!(result.classes.contains(&ChangeClass::CiPolicy), "{path}");
    }
}

#[test]
fn product_classes_retain_fast_local_coverage() {
    let terminal = paths(&["src/ui/render.rs"]);
    assert_eq!(terminal.local_plan, LocalPlan::Fast);
    assert!(terminal.coverage);
    assert!(terminal.classes.contains(&ChangeClass::Terminal));

    let persistence = paths(&["src/adapters/sqlite/migration.rs"]);
    assert_eq!(persistence.local_plan, LocalPlan::Fast);
    assert!(
        persistence
            .classes
            .contains(&ChangeClass::PersistenceContract)
    );
}

#[test]
fn dependency_package_release_and_unknown_paths_use_full_gate() {
    for path in [
        "Cargo.lock",
        "xtask/src/debian_verify.rs",
        ".github/release-notes/v1.2.3.md",
        "unowned.future",
    ] {
        assert_eq!(paths(&[path]).local_plan, LocalPlan::Full, "{path}");
    }
}

#[test]
fn empty_diff_is_not_documentation_and_uses_full_gate() {
    let result = paths(&[]);
    assert!(!result.docs_only);
    assert_eq!(result.local_plan, LocalPlan::Full);
    assert!(result.full_msrv);
    assert!(result.classes.contains(&ChangeClass::Unknown));
}

#[test]
fn schema_keeps_required_fields_and_advisory_data() {
    let result = paths(&["src/ui/render.rs", "Cargo.lock"]);
    let value = classification_json(&result, "base", "head");
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["base_sha"], "base");
    assert_eq!(value["head_sha"], "head");
    assert_eq!(value["coverage"], true);
    assert_eq!(value["full_msrv"], true);
    assert_eq!(value["local_plan"], "full");
    assert_eq!(value["advisory"]["pty"], "required");
}

#[test]
fn high_risk_shadow_advice_never_recommends_a_skip() {
    let result = paths(&[".github/workflows/ci.yml"]);
    let advisory = classification_json(&result, "base", "head")["advisory"]
        .as_object()
        .expect("advisory object")
        .clone();
    assert!(advisory.values().all(|value| value == "required"));
}

#[test]
fn local_diff_includes_committed_dirty_deleted_and_untracked_paths() {
    let temporary = tempfile::tempdir().expect("repository root");
    let root = temporary.path();
    run(root, ["init", "-q", "-b", "main"]);
    run(root, ["config", "user.name", "Test"]);
    run(root, ["config", "user.email", "test@example.invalid"]);
    fs::write(root.join("tracked.md"), "tracked\n").expect("tracked");
    fs::write(root.join("deleted.md"), "deleted\n").expect("deleted");
    fs::create_dir_all(root.join(".github/workflows")).expect("workflow directory");
    fs::write(root.join(".github/workflows/policy.yml"), "name: policy\n").expect("policy");
    run(root, ["add", "."]);
    run(root, ["commit", "-qm", "base"]);
    run(root, ["switch", "-qc", "feature"]);
    fs::write(root.join("tracked.md"), "changed\n").expect("changed");
    fs::remove_file(root.join("deleted.md")).expect("delete");
    run(
        root,
        ["mv", ".github/workflows/policy.yml", "renamed-policy.md"],
    );
    fs::write(root.join("untracked.md"), "new\n").expect("untracked");
    let changes = local(root, Some("main")).expect("local changes");
    assert!(changes.paths.contains(&"tracked.md".to_owned()));
    assert!(changes.paths.contains(&"deleted.md".to_owned()));
    assert!(changes.paths.contains(&"untracked.md".to_owned()));
    assert!(
        changes
            .paths
            .contains(&".github/workflows/policy.yml".to_owned())
    );
    assert!(changes.paths.contains(&"renamed-policy.md".to_owned()));
    assert!(changes.untracked.contains("untracked.md"));
    assert_eq!(changes.classification.local_plan, LocalPlan::Full);
    assert!(local(root, Some("missing-revision")).is_err());
}

fn run<const N: usize>(root: &Path, arguments: [&str; N]) {
    assert!(
        Command::new("git")
            .args(arguments)
            .current_dir(root)
            .status()
            .expect("git")
            .success()
    );
}
