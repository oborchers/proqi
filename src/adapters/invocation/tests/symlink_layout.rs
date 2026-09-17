//! Cross-harness ownership for supported global skill symlink layouts.

use std::{fs, os::unix::fs::symlink};

use crate::ports::invocation::{
    InvocationCatalog as _, InvocationDiscoveryRequest, InvocationHarness, InvocationKind,
    InvocationScope,
};

use super::{FilesystemInvocationCatalog, discover, write};

#[test]
fn global_external_agent_skill_retains_both_forms_below_non_git_home() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = home.join("projects/non-git/nested");
    let external = home.join("agent-os/skills/example");
    fs::create_dir_all(&cwd).expect("nested non-Git cwd");
    write(
        &external.join("SKILL.md"),
        "---\nname: aos-example\ndescription: Synthetic Grüße 界\n---\nbody",
    );
    fs::create_dir_all(home.join(".agents/skills")).expect("agent skills root");
    fs::create_dir_all(home.join(".claude/skills")).expect("Claude skills root");
    symlink(
        "../../agent-os/skills/example",
        home.join(".agents/skills/aos-example"),
    )
    .expect("external Agent Skills alias");
    symlink(
        "../../.agents/skills/aos-example",
        home.join(".claude/skills/aos-example"),
    )
    .expect("Claude alias through Agent Skills");

    let result = discover(&home, &cwd);

    assert!(result.project.is_empty());
    let [entry] = result.global.as_slice() else {
        panic!("one consolidated global skill");
    };
    assert_eq!(entry.scope, InvocationScope::Global);
    assert_eq!(entry.source, InvocationHarness::AgentSkills);
    assert_eq!(entry.description.as_deref(), Some("Synthetic Grüße 界"));
    assert_eq!(
        entry
            .forms
            .iter()
            .map(|form| (form.token.as_str(), form.harness, form.precedence))
            .collect::<Vec<_>>(),
        vec![
            ("/aos-example", InvocationHarness::ClaudeCode, 5),
            ("$aos-example", InvocationHarness::Codex, 40),
        ]
    );
}

#[test]
fn global_scope_stays_stable_at_home_git_and_worktree_boundaries() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let external = home.join("agent-os/skills/shared");
    write(
        &external.join("SKILL.md"),
        "---\nname: shared\ndescription: Global skill\n---\nbody",
    );
    fs::create_dir_all(home.join(".agents/skills")).expect("agent skills root");
    fs::create_dir_all(home.join(".claude/skills")).expect("Claude skills root");
    symlink(
        "../../agent-os/skills/shared",
        home.join(".agents/skills/shared"),
    )
    .expect("external Agent Skills alias");
    symlink(
        "../../.agents/skills/shared",
        home.join(".claude/skills/shared"),
    )
    .expect("Claude alias through Agent Skills");

    let repository = home.join("projects/repository");
    let worktree = home.join("projects/worktree");
    write(&repository.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&worktree.join(".git"), "gitdir: /fixture/repository.git\n");
    for cwd in [home.clone(), repository, worktree] {
        let result = discover(&home, &cwd);
        assert!(result.project.is_empty(), "cwd={}", cwd.display());
        let [entry] = result.global.as_slice() else {
            panic!("one global skill for cwd={}", cwd.display());
        };
        assert_eq!(entry.scope, InvocationScope::Global);
        assert_eq!(entry.precedence, 5);
        assert_eq!(
            entry
                .forms
                .iter()
                .map(|form| form.token.as_str())
                .collect::<Vec<_>>(),
            vec!["/shared", "$shared"]
        );
    }
}

#[test]
fn project_only_home_roots_survive_non_git_and_home_repository_contexts() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let nested = home.join("projects/non-git");
    write(
        &home.join(".pi/skills/home-skill/SKILL.md"),
        "---\nname: home-skill\ndescription: Project-only home skill\n---\nbody",
    );
    write(
        &home.join(".opencode/commands/home-command.md"),
        "---\ndescription: Project-only home command\n---\nbody",
    );
    fs::create_dir_all(&nested).expect("nested non-Git cwd");

    let nested_result = discover(&home, &nested);
    assert!(nested_result.global.is_empty());
    assert_eq!(
        nested_result
            .project
            .iter()
            .flat_map(|entry| entry.forms.iter().map(|form| form.token.as_str()))
            .collect::<Vec<_>>(),
        vec!["/home-command", "/skill:home-skill"]
    );

    write(&home.join(".git/HEAD"), "ref: refs/heads/main\n");
    let repository_result = discover(&home, &home);
    assert!(repository_result.global.is_empty());
    assert_eq!(
        repository_result
            .project
            .iter()
            .flat_map(|entry| entry.forms.iter().map(|form| form.token.as_str()))
            .collect::<Vec<_>>(),
        vec!["/home-command", "/skill:home-skill"]
    );
}

#[test]
fn distinct_project_and_global_same_name_skills_do_not_share_forms() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("outside-home/repository");
    write(&cwd.join(".git"), "gitdir: /fixture/repository.git\n");
    write(
        &cwd.join(".agents/skills/shared/SKILL.md"),
        "---\nname: shared\ndescription: Project skill\n---\nbody",
    );
    write(
        &home.join(".agents/skills/shared/SKILL.md"),
        "---\nname: shared\ndescription: Global skill\n---\nbody",
    );
    fs::create_dir_all(home.join(".claude/skills")).expect("Claude skills root");
    symlink(
        "../../.agents/skills/shared",
        home.join(".claude/skills/shared"),
    )
    .expect("global Claude alias through Agent Skills");

    let result = discover(&home, &cwd);

    let [project] = result.project.as_slice() else {
        panic!("one project definition");
    };
    assert_eq!(project.description.as_deref(), Some("Project skill"));
    assert_eq!(project.forms.len(), 1);
    assert_eq!(project.forms[0].token, "$shared");
    let [global] = result.global.as_slice() else {
        panic!("one global definition");
    };
    assert_eq!(global.description.as_deref(), Some("Global skill"));
    assert_eq!(
        global
            .forms
            .iter()
            .map(|form| form.token.as_str())
            .collect::<Vec<_>>(),
        vec!["/shared", "$shared"]
    );
    assert_ne!(project.canonical_path, global.canonical_path);
}

#[test]
fn external_agent_skill_with_a_claude_alias_retains_both_harness_forms() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let external = home.join("agent-os/skills/media/aos-media-image");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &external.join("SKILL.md"),
        "---\nname: aos-media-image\ndescription: Erstellt Grüße für 界\n---\nbody",
    );
    fs::create_dir_all(home.join(".agents/skills")).expect("agent skills root");
    fs::create_dir_all(home.join(".claude/skills")).expect("Claude skills root");
    let agent_alias = home.join(".agents/skills/aos-media-image");
    symlink("../../agent-os/skills/media/aos-media-image", &agent_alias)
        .expect("external Agent Skills alias");
    symlink(
        "../../.agents/skills/aos-media-image",
        home.join(".claude/skills/aos-media-image"),
    )
    .expect("Claude alias through Agent Skills");

    let result = discover(&home, &cwd);
    let [entry] = result.global.as_slice() else {
        panic!("one consolidated global skill");
    };
    assert_eq!(entry.kind, InvocationKind::Skill);
    assert_eq!(entry.source, InvocationHarness::AgentSkills);
    assert_eq!(
        entry.canonical_path,
        fs::canonicalize(external.join("SKILL.md")).expect("canonical external definition")
    );
    assert_eq!(entry.description.as_deref(), Some("Erstellt Grüße für 界"));
    assert_eq!(
        entry
            .forms
            .iter()
            .map(|form| (form.token.as_str(), form.harness, form.precedence))
            .collect::<Vec<_>>(),
        vec![
            ("/aos-media-image", InvocationHarness::ClaudeCode, 5),
            ("$aos-media-image", InvocationHarness::Codex, 40),
        ]
    );
}

#[test]
fn root_and_skill_symlinks_consolidate_without_duplicate_or_unsupported_forms() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let external = fixture.path().join("catalog/shared");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &external.join("SKILL.md"),
        "---\nname: shared\ndescription: Shared skill\n---\nbody",
    );
    fs::create_dir_all(home.join(".agents/skills")).expect("agent skills root");
    symlink(&external, home.join(".agents/skills/one")).expect("first agent alias");
    symlink(&external, home.join(".agents/skills/two")).expect("second agent alias");
    fs::create_dir_all(home.join(".claude")).expect("Claude root");
    symlink(home.join(".agents/skills"), home.join(".claude/skills")).expect("Claude root alias");
    fs::create_dir_all(home.join(".cursor/skills")).expect("catalog-only root");
    symlink(&external, home.join(".cursor/skills/shared")).expect("unsupported alias");

    let result = discover(&home, &cwd);
    let [entry] = result.global.as_slice() else {
        panic!("one canonical skill from every supported alias");
    };
    assert_eq!(entry.source, InvocationHarness::AgentSkills);
    assert_eq!(
        entry
            .forms
            .iter()
            .map(|form| (form.token.as_str(), form.harness))
            .collect::<Vec<_>>(),
        vec![
            ("/shared", InvocationHarness::ClaudeCode),
            ("$shared", InvocationHarness::Codex),
        ]
    );
}

#[test]
fn missing_broken_and_cyclic_skill_links_are_ignored() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    fs::create_dir_all(home.join(".agents/skills")).expect("agent skills root");
    fs::create_dir_all(home.join(".claude/skills")).expect("Claude skills root");
    symlink(
        fixture.path().join("missing"),
        home.join(".agents/skills/broken"),
    )
    .expect("broken alias");
    symlink("cycle-b", home.join(".agents/skills/cycle-a")).expect("first cycle alias");
    symlink("cycle-a", home.join(".agents/skills/cycle-b")).expect("second cycle alias");
    symlink(
        home.join(".agents/skills/broken"),
        home.join(".claude/skills/broken"),
    )
    .expect("chained broken alias");

    let result = discover(&home, &cwd);
    assert!(result.global.is_empty());
    assert!(result.project.is_empty());
}

#[test]
fn repeated_discovery_refreshes_new_external_skill_aliases() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    fs::create_dir_all(home.join(".agents/skills")).expect("agent skills root");
    fs::create_dir_all(home.join(".claude/skills")).expect("Claude skills root");
    let mut catalog = FilesystemInvocationCatalog::with_home(Some(home.clone()), Vec::new());
    let request = |generation| InvocationDiscoveryRequest {
        generation,
        cwd: cwd.clone(),
    };

    let initial = catalog.discover(request(1));
    assert!(initial.global.is_empty());

    let external = home.join("agent-os/skills/newly-installed");
    write(
        &external.join("SKILL.md"),
        "---\nname: newly-installed\ndescription: Available after refresh\n---\nbody",
    );
    symlink(
        "../../agent-os/skills/newly-installed",
        home.join(".agents/skills/newly-installed"),
    )
    .expect("new Agent Skills alias");
    symlink(
        "../../.agents/skills/newly-installed",
        home.join(".claude/skills/newly-installed"),
    )
    .expect("new Claude alias");

    let refreshed = catalog.discover(request(2));
    let [entry] = refreshed.global.as_slice() else {
        panic!("one newly discovered global skill");
    };
    assert_eq!(refreshed.generation, 2);
    assert_eq!(
        entry
            .forms
            .iter()
            .map(|form| (form.token.as_str(), form.harness))
            .collect::<Vec<_>>(),
        vec![
            ("/newly-installed", InvocationHarness::ClaudeCode),
            ("$newly-installed", InvocationHarness::Codex),
        ]
    );
}
