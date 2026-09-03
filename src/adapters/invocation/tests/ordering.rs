//! Ordering compatibility when a shared work dimension is exhausted.

use crate::ports::invocation::InvocationIncompleteReason;

use super::{discover, write};

#[test]
fn plugin_precedence_still_owns_the_entry_budget_before_a_deep_project_root() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let repo = fixture.path().join("repo");
    let plugin = fixture.path().join("plugin");
    write(&repo.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &repo.join(".opencode/commands/project.md"),
        "---\ndescription: Project command\n---\nbody",
    );
    for index in 0..2_048 {
        write(
            &plugin.join(format!("commands/plugin-{index:04}.md")),
            "---\ndescription: Plugin command\n---\nbody",
        );
    }
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &serde_json::json!({"plugins":{"ordered@catalog":[{"installPath":plugin}]}}).to_string(),
    );
    let cwd = (0..65).fold(repo, |path, index| path.join(format!("d{index}")));
    std::fs::create_dir_all(&cwd).expect("deep cwd");

    let result = discover(&home, &cwd);

    assert_eq!(result.global.len(), 2_048);
    assert!(result.project.is_empty());
    assert!(matches!(
        result.completeness.reasons(),
        [InvocationIncompleteReason::EntryBudget {
            observed: 2_049,
            limit: 2_048
        }]
    ));
}
