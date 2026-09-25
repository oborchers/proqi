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
fn documentation_site_inputs_reach_the_documentation_gate() {
    for path in [
        "mkdocs.yml",
        ".github/workflows/ci.yml",
        ".github/workflows/docs.yml",
        "docs/stylesheets/extra.css",
        "docs/pyproject.toml",
        "docs/uv.lock",
        "docs/check.py",
        "docs/check_site.py",
        "src/cli/args.rs",
        "src/ui/shortcut_registry/model.rs",
        "xtask/src/documentation.rs",
    ] {
        let result = paths(&[path]);
        assert!(
            result.classes.contains(&ChangeClass::Documentation),
            "{path}"
        );
        assert!(!result.docs_only, "{path}");
    }
}

#[test]
fn policy_and_classifier_changes_fail_closed() {
    for path in [
        ".github/workflows/ci.yml",
        ".github/workflows/docs.yml",
        "AGENTS.md",
        "context/ARCHITECTURE.md",
        "xtask/src/ci_changes.rs",
        "xtask/src/ci_changes/git.rs",
        "xtask/src/dev_gates.rs",
        "xtask/src/documentation.rs",
        "xtask/src/release_policy/docs.rs",
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
fn shipped_skills_leave_the_documentation_path_without_hiding_it() {
    for path in [
        "skills/proqi/SKILL.md",
        "skills/proqi-debug/references/storage.md",
        "skills/proqi/agents/openai.yaml",
    ] {
        let result = paths(&[path]);
        assert!(!result.docs_only, "{path}");
        assert_eq!(result.local_plan, LocalPlan::Fast, "{path}");
        assert!(result.classes.contains(&ChangeClass::Product), "{path}");
        assert!(!result.classes.contains(&ChangeClass::Unknown), "{path}");
    }
    let docs = paths(&[
        "README.md",
        "docs/getting-started.md",
        "docs/guides/updates.md",
    ]);
    assert!(docs.docs_only);
    assert_eq!(docs.local_plan, LocalPlan::Documentation);
    let mixed = paths(&["docs/reference/cli.md", "skills/proqi/SKILL.md"]);
    assert!(!mixed.docs_only);
    assert_eq!(mixed.local_plan, LocalPlan::Fast);
}

#[test]
fn claude_marketplace_manifest_and_policy_use_full_gate() {
    for path in [
        ".claude-plugin/marketplace.json",
        "xtask/src/claude_marketplace.rs",
    ] {
        let result = paths(&[path]);
        assert_eq!(result.local_plan, LocalPlan::Full, "{path}");
        assert!(result.full_msrv, "{path}");
        assert!(!result.classes.contains(&ChangeClass::Unknown), "{path}");
    }
    assert!(
        paths(&[".claude-plugin/marketplace.json"])
            .classes
            .contains(&ChangeClass::Packaging)
    );
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

#[test]
fn herdr_plugin_files_are_packaging_and_run_the_full_plan() {
    for path in [
        "herdr-plugin.toml",
        "herdr-plugin/proqi.sh",
        "herdr-plugin/install.sh",
    ] {
        let result = paths(&[path]);
        assert!(result.classes.contains(&ChangeClass::Packaging), "{path}");
        assert!(!result.classes.contains(&ChangeClass::Unknown), "{path}");
        assert_eq!(result.local_plan, LocalPlan::Full, "{path}");
    }
}

#[test]
fn lookalike_herdr_plugin_paths_remain_unknown() {
    for path in [
        "herdr-plugin.json",
        "herdr-plugins/proqi.sh",
        "herdr-plugin.toml.bak",
    ] {
        let result = paths(&[path]);
        assert!(result.classes.contains(&ChangeClass::Unknown), "{path}");
        assert!(!result.classes.contains(&ChangeClass::Packaging), "{path}");
    }
}
