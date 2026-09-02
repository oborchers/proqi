use std::{fs, path::Path};

use tempfile::TempDir;

use crate::ports::invocation::{
    InvocationCatalog as _, InvocationDiscoveryRequest, InvocationHarness, InvocationKind,
    InvocationScope,
};

use super::FilesystemInvocationCatalog;

#[cfg(unix)]
mod symlink_layout;

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture directory");
    fs::write(path, content).expect("write fixture");
}

fn discover(home: &Path, cwd: &Path) -> crate::ports::invocation::InvocationDiscovery {
    FilesystemInvocationCatalog::with_home(Some(home.to_owned()), Vec::new())
        .discover(InvocationDiscoveryRequest {
            generation: 7,
            cwd: cwd.to_owned(),
        })
        .expect("discover fixtures")
}

#[test]
fn globals_are_independent_while_project_entries_follow_the_cwd() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let first = fixture.path().join("first/repo/nested");
    let second = fixture.path().join("second/repo");
    write(&first.join("../.git/HEAD"), "ref: refs/heads/main\n");
    write(&second.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &home.join(".agents/skills/global-one/SKILL.md"),
        "---\nname: global-one\ndescription: Shared everywhere\n---\nprivate body",
    );
    write(
        &first.join("../.claude/commands/api-check.md"),
        "---\ndescription: Check an API\n---\nprivate body",
    );
    write(
        &second.join(".claude/agents/reviewer.md"),
        "---\nname: reviewer\ndescription: Review changes\n---\nprivate body",
    );

    let left = discover(&home, &first);
    let right = discover(&home, &second);

    assert_eq!(left.global, right.global);
    assert_eq!(left.global[0].forms[0].token, "$global-one");
    assert_eq!(left.global[0].forms[0].harness, InvocationHarness::Codex);
    assert_eq!(left.project[0].forms[0].token, "/api-check");
    assert_eq!(right.project[0].forms[0].token, "@agent-reviewer");
}

#[test]
fn kinds_and_documented_harness_forms_stay_distinct() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &cwd.join(".claude/skills/plan/SKILL.md"),
        "---\nname: plan\ndescription: Plan work\n---\nbody",
    );
    write(
        &cwd.join(".claude/commands/plan.md"),
        "---\ndescription: Run a plan\n---\nbody",
    );
    write(
        &cwd.join(".claude/agents/plan.md"),
        "---\nname: plan\ndescription: Planning agent\n---\nbody",
    );

    let result = discover(&home, &cwd);
    let summary = result
        .project
        .iter()
        .map(|entry| (entry.kind, entry.forms[0].token.as_str()))
        .collect::<Vec<_>>();
    assert!(summary.contains(&(InvocationKind::Skill, "/plan")));
    assert!(summary.contains(&(InvocationKind::Command, "/plan")));
    assert!(summary.contains(&(InvocationKind::Agent, "@agent-plan")));
}

#[test]
fn symlinks_are_canonicalized_and_invalid_or_large_metadata_is_skipped() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let shared = fixture.path().join("shared/SKILL.md");
    write(&shared, "---\nname: linked\ndescription: Safe\n---\nbody");
    let skills = cwd.join(".agents/skills");
    let claude_skills = cwd.join(".claude/skills");
    fs::create_dir_all(&skills).expect("skills root");
    fs::create_dir_all(&claude_skills).expect("claude skills root");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(shared.parent().expect("shared parent"), skills.join("one"))
            .expect("first symlink");
        std::os::unix::fs::symlink(shared.parent().expect("shared parent"), skills.join("two"))
            .expect("second symlink");
        std::os::unix::fs::symlink(
            shared.parent().expect("shared parent"),
            claude_skills.join("cross-harness"),
        )
        .expect("cross-harness symlink");
    }
    write(
        &cwd.join(".claude/agents/control.md"),
        "---\nname: bad\u{0007}name\ndescription: no\n---\nbody",
    );
    write(
        &cwd.join(".claude/commands/huge.md"),
        &"x".repeat(17 * 1024),
    );

    let result = discover(&home, &cwd);
    assert_eq!(
        result
            .project
            .iter()
            .filter(|entry| entry.name == "linked")
            .count(),
        1
    );
    let linked = result
        .project
        .iter()
        .find(|entry| entry.name == "linked")
        .expect("linked skill");
    assert_eq!(linked.forms.len(), 1);
    assert_eq!(linked.forms[0].token, "$linked");
    assert!(result.project.iter().all(|entry| entry.name != "huge"));
    assert!(
        result
            .project
            .iter()
            .all(|entry| !entry.name.contains(char::is_control))
    );
}

#[cfg(unix)]
#[test]
fn claude_skill_symlink_into_project_agents_root_retains_both_forms() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let agent_skill = cwd.join(".agents/skills/shared");
    write(
        &agent_skill.join("SKILL.md"),
        "---\nname: shared\ndescription: Shared skill\n---\nbody",
    );
    fs::create_dir_all(cwd.join(".claude/skills")).expect("claude skills root");
    std::os::unix::fs::symlink(&agent_skill, cwd.join(".claude/skills/shared"))
        .expect("claude skill alias");

    let result = discover(&home, &cwd);
    let [entry] = result.project.as_slice() else {
        panic!("one shared project skill");
    };
    assert_eq!(entry.source, InvocationHarness::AgentSkills);
    assert_eq!(entry.precedence, 20);
    assert_eq!(
        entry
            .forms
            .iter()
            .map(|form| (form.token.as_str(), form.harness, form.precedence))
            .collect::<Vec<_>>(),
        vec![
            ("$shared", InvocationHarness::Codex, 20),
            ("/shared", InvocationHarness::ClaudeCode, 25),
        ]
    );
}

#[cfg(unix)]
#[test]
fn symlinked_global_claude_skills_root_retains_harness_precedence() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &home.join(".agents/skills/shared/SKILL.md"),
        "---\nname: shared\ndescription: Shared skill\n---\nbody",
    );
    fs::create_dir_all(home.join(".claude")).expect("claude root");
    std::os::unix::fs::symlink(home.join(".agents/skills"), home.join(".claude/skills"))
        .expect("claude skills alias");

    let result = discover(&home, &cwd);
    let [entry] = result.global.as_slice() else {
        panic!("one shared global skill");
    };
    assert_eq!(entry.source, InvocationHarness::AgentSkills);
    assert_eq!(entry.precedence, 5);
    assert_eq!(entry.forms[0].token, "/shared");
    assert_eq!(entry.forms[0].precedence, 5);
    assert_eq!(entry.forms[1].token, "$shared");
    assert_eq!(entry.forms[1].precedence, 40);
}

#[test]
fn copied_agent_and_claude_skills_remain_independent() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    for root in [".agents/skills", ".claude/skills"] {
        write(
            &cwd.join(root).join("shared/SKILL.md"),
            "---\nname: shared\ndescription: Shared skill\n---\nbody",
        );
    }

    let result = discover(&home, &cwd);
    assert_eq!(result.project.len(), 2);
    assert_ne!(
        result.project[0].canonical_path,
        result.project[1].canonical_path
    );
    assert_eq!(result.project[0].forms[0].token, "$shared");
    assert_eq!(result.project[1].forms[0].token, "/shared");
}

#[cfg(unix)]
#[test]
fn reverse_agent_symlink_does_not_fabricate_a_shared_skill() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let claude_skill = cwd.join(".claude/skills/shared");
    write(
        &claude_skill.join("SKILL.md"),
        "---\nname: shared\ndescription: Shared skill\n---\nbody",
    );
    fs::create_dir_all(cwd.join(".agents/skills")).expect("agent skills root");
    std::os::unix::fs::symlink(&claude_skill, cwd.join(".agents/skills/shared"))
        .expect("reverse alias");

    let result = discover(&home, &cwd);
    let [entry] = result.project.as_slice() else {
        panic!("one canonicalized skill");
    };
    assert_eq!(entry.forms.len(), 1);
    assert_eq!(entry.forms[0].token, "$shared");
}

#[test]
fn codex_agents_are_catalogued_without_fabricating_an_invocation() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &cwd.join(".codex/agents/explorer.toml"),
        "name = \"explorer\"\ndescription = \"Explore code\"\ndeveloper_instructions = \"private\"\n",
    );

    let result = discover(&home, &cwd);
    let entry = result.project.first().expect("codex agent");
    assert_eq!(entry.kind, InvocationKind::Agent);
    assert_eq!(entry.source, InvocationHarness::Codex);
    assert_eq!(entry.scope, InvocationScope::Project);
    assert!(entry.forms.is_empty());
}

#[test]
fn npx_skills_compatibility_roots_without_documented_tokens_are_catalog_only() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &home.join(".cursor/skills/portable/SKILL.md"),
        "---\nname: portable\ndescription: Portable metadata\n---\nbody",
    );

    let result = discover(&home, &cwd);
    let entry = result.global.first().expect("portable skill");
    assert_eq!(entry.kind, InvocationKind::Skill);
    assert_eq!(entry.source, InvocationHarness::AgentSkills);
    assert!(entry.forms.is_empty());
}

#[test]
fn installed_plugin_manifest_paths_keep_namespaced_command_and_agent_forms() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let plugin = fixture.path().join("plugins/quality");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &plugin.join(".claude-plugin/plugin.json"),
        "{\"name\":\"quality\",\"skills\":\"abilities/audit\",\"commands\":\"definitions/check.md\",\"agents\":[\"people/reviewer.md\"]}",
    );
    write(
        &plugin.join("abilities/audit/SKILL.md"),
        "---\nname: audit\ndescription: Audit work\n---\nbody",
    );
    write(
        &plugin.join("definitions/check.md"),
        "---\ndescription: Check work\n---\nbody",
    );
    write(
        &plugin.join("people/reviewer.md"),
        "---\nname: reviewer\ndescription: Review work\n---\nbody",
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            "{{\"version\":2,\"plugins\":{{\"quality@catalog\":[{{\"installPath\":{:?}}}]}}}}",
            plugin.to_string_lossy()
        ),
    );

    let result = discover(&home, &cwd);
    let tokens = result
        .global
        .iter()
        .flat_map(|entry| entry.forms.iter().map(|form| form.token.as_str()))
        .collect::<Vec<_>>();
    assert!(tokens.contains(&"/quality:check"));
    assert!(tokens.contains(&"/quality:audit"));
    assert!(tokens.contains(&"@agent-quality:reviewer"));
    assert!(
        result
            .global
            .iter()
            .all(|entry| entry.scope == InvocationScope::Plugin)
    );
}

#[test]
fn project_scoped_plugins_do_not_leak_into_an_unrelated_cwd() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("current");
    let other = fixture.path().join("other");
    let plugin = fixture.path().join("plugins/project-only");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&other.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &plugin.join("commands/only-here.md"),
        "---\ndescription: Project command\n---\nbody",
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            "{{\"plugins\":{{\"local@catalog\":[{{\"installPath\":{:?},\"scope\":\"project\",\"projectPath\":{:?}}}]}}}}",
            plugin.to_string_lossy(),
            other.to_string_lossy()
        ),
    );

    let result = discover(&home, &cwd);
    assert!(result.global.iter().all(|entry| entry.name != "only-here"));
}

#[test]
fn unicode_descriptions_are_single_line_and_controls_are_removed() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &cwd.join(".agents/skills/gruesse/SKILL.md"),
        "---\nname: gruesse\ndescription: Grüße\t界  👩‍💻\n---\nprivate body",
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        "{not json",
    );

    let result = discover(&home, &cwd);
    assert_eq!(
        result.project[0].description.as_deref(),
        Some("Grüße 界 👩‍💻")
    );
}

#[test]
fn oversized_catalogs_stop_at_the_fixed_entry_budget() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    for index in 0..2_055 {
        write(
            &cwd.join(format!(".opencode/commands/command-{index:04}.md")),
            "---\ndescription: Bounded fixture\n---\nbody",
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            fixture.path().join("missing.md"),
            cwd.join(".opencode/commands/broken.md"),
        )
        .expect("broken symlink fixture");
    }

    let result = discover(&home, &cwd);
    assert_eq!(result.project.len(), 2_048);
}
