use std::{fs, path::Path};

use super::{discover, write};
use tempfile::TempDir;

fn large_markdown(frontmatter: &str, total_bytes: usize) -> String {
    let mut definition = format!("---\n{frontmatter}---\n");
    assert!(definition.len() < total_bytes);
    definition.push_str(&"x".repeat(total_bytes.saturating_sub(definition.len())));
    definition
}

fn write_bytes(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture directory");
    fs::write(path, content).expect("write fixture");
}

fn has_token(result: &crate::ports::invocation::InvocationDiscovery, token: &str) -> bool {
    result
        .project
        .iter()
        .chain(&result.global)
        .any(|entry| entry.forms.iter().any(|form| form.token == token))
}

fn issue_51_command() -> String {
    const FRONTMATTER_END: usize = 614;
    const TOTAL_BYTES: usize = 19_702;
    let mut definition =
        String::from("---\ndescription: Issue 51 regression command\nmetadata-padding: ");
    let padding = FRONTMATTER_END
        .saturating_sub(definition.len())
        .saturating_sub("\n---\n".len());
    definition.push_str(&"x".repeat(padding));
    definition.push_str("\n---\n");
    assert_eq!(definition.len(), FRONTMATTER_END);
    definition.push_str(&"x".repeat(TOTAL_BYTES.saturating_sub(definition.len())));
    assert_eq!(definition.len(), TOTAL_BYTES);
    definition
}

#[test]
fn issue_51_large_plugin_command_uses_its_small_frontmatter() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let plugin = fixture.path().join("plugins/issue51");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(&plugin.join("commands/large.md"), &issue_51_command());
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            "{{\"plugins\":{{\"issue51@catalog\":[{{\"installPath\":{:?}}}]}}}}",
            plugin.to_string_lossy()
        ),
    );

    let result = discover(&home, &cwd);

    assert!(result.global.iter().any(|entry| {
        entry
            .forms
            .iter()
            .any(|form| form.token == "/issue51:large")
    }));
}

#[test]
fn large_markdown_definitions_work_across_scope_and_kind_boundaries() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let plugin = fixture.path().join("plugins/stress");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");

    let definitions = [
        (
            cwd.join(".agents/skills/project-skill/SKILL.md"),
            "name: project-skill\ndescription: Project Agent Skill\n",
            24_799,
        ),
        (
            home.join(".agents/skills/global-skill/SKILL.md"),
            "name: global-skill\ndescription: Global Agent Skill\n",
            96 * 1024,
        ),
        (
            plugin.join("skills/plugin-skill/SKILL.md"),
            "name: plugin-skill\ndescription: Plugin skill\n",
            128 * 1024,
        ),
        (
            cwd.join(".claude/commands/project-command.md"),
            "description: Project command\n",
            32 * 1024,
        ),
        (
            home.join(".claude/commands/global-command.md"),
            "description: Global command\n",
            256 * 1024,
        ),
        (
            cwd.join(".claude/agents/project-agent.md"),
            "name: project-agent\ndescription: Project agent\n",
            48 * 1024,
        ),
        (
            home.join(".claude/agents/global-agent.md"),
            "name: global-agent\ndescription: Global agent\n",
            512 * 1024,
        ),
        (
            plugin.join("agents/plugin-agent.md"),
            "name: plugin-agent\ndescription: Plugin agent\n",
            8 * 1024 * 1024,
        ),
    ];
    for (path, frontmatter, size) in definitions {
        write(&path, &large_markdown(frontmatter, size));
    }
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            "{{\"plugins\":{{\"stress@catalog\":[{{\"installPath\":{:?}}}]}}}}",
            plugin.to_string_lossy()
        ),
    );

    let result = discover(&home, &cwd);

    for token in [
        "$project-skill",
        "$global-skill",
        "/stress:plugin-skill",
        "/project-command",
        "/global-command",
        "@agent-project-agent",
        "@agent-global-agent",
        "@agent-stress:plugin-agent",
    ] {
        assert!(has_token(&result, token), "missing {token}");
    }
}

#[cfg(unix)]
#[test]
fn large_symlinked_skill_keeps_agent_and_claude_forms() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let external = fixture.path().join("external/shared");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &external.join("SKILL.md"),
        &large_markdown(
            "name: shared-large\ndescription: Large shared symlink\n",
            2 * 1024 * 1024,
        ),
    );
    fs::create_dir_all(cwd.join(".agents/skills")).expect("Agent Skills root");
    fs::create_dir_all(cwd.join(".claude/skills")).expect("Claude skills root");
    let agent_alias = cwd.join(".agents/skills/shared-large");
    std::os::unix::fs::symlink(&external, &agent_alias).expect("Agent Skills alias");
    std::os::unix::fs::symlink(&agent_alias, cwd.join(".claude/skills/shared-large"))
        .expect("Claude alias");

    let result = discover(&home, &cwd);

    assert!(has_token(&result, "$shared-large"));
    assert!(has_token(&result, "/shared-large"));
    assert_eq!(
        result
            .project
            .iter()
            .filter(|entry| entry.name == "shared-large")
            .count(),
        1
    );
}

#[test]
fn metadata_validity_and_visibility_are_decided_before_large_bodies() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write(
        &cwd.join(".agents/skills/unicode/SKILL.md"),
        &large_markdown("name: unicode\ndescription: Grüße 界 und 👩‍💻\n", 80 * 1024),
    );
    let crlf = large_markdown("name: crlf-agent\ndescription: CRLF agent\n", 72 * 1024)
        .replace('\n', "\r\n");
    write(&cwd.join(".claude/agents/crlf-agent.md"), &crlf);
    write(
        &cwd.join(".claude/commands/hidden.md"),
        &large_markdown("description: Hidden\nhidden: true\n", 96 * 1024),
    );
    write(
        &cwd.join(".agents/skills/disabled/SKILL.md"),
        &large_markdown(
            "name: disabled\ndescription: Disabled\ndisable: yes\n",
            96 * 1024,
        ),
    );

    let result = discover(&home, &cwd);

    assert!(has_token(&result, "$unicode"));
    assert!(has_token(&result, "@agent-crlf-agent"));
    assert!(!has_token(&result, "/hidden"));
    assert!(!has_token(&result, "$disabled"));
    let unicode = result
        .project
        .iter()
        .find(|entry| entry.name == "unicode")
        .expect("Unicode skill");
    assert_eq!(unicode.description.as_deref(), Some("Grüße 界 und 👩‍💻"));
}

#[test]
fn missing_or_invalid_headers_keep_filename_commands_but_reject_required_metadata() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    write_bytes(
        &cwd.join(".claude/commands/no-header.md"),
        b"\xff\xfe instruction body without frontmatter",
    );
    write(
        &cwd.join(".agents/skills/no-header/SKILL.md"),
        "instruction body without frontmatter",
    );
    write(
        &cwd.join(".claude/agents/malformed.md"),
        "---\nnot valid metadata\n---\nbody",
    );
    write_bytes(
        &cwd.join(".agents/skills/invalid-utf8/SKILL.md"),
        b"---\nname: invalid-utf8\ndescription: bad \xff\n---\nbody",
    );
    write_bytes(
        &cwd.join(".claude/commands/invalid-utf8-header.md"),
        b"---\ndescription: bad \xff\n---\nbody",
    );
    let mut invalid_body = b"---\nname: valid-body\ndescription: Metadata is valid\n---\n".to_vec();
    invalid_body.extend_from_slice(b"\xff\xfe body bytes");
    write_bytes(
        &cwd.join(".agents/skills/valid-body/SKILL.md"),
        &invalid_body,
    );

    let result = discover(&home, &cwd);

    assert!(has_token(&result, "/no-header"));
    assert!(has_token(&result, "$valid-body"));
    assert!(!has_token(&result, "$no-header"));
    assert!(!has_token(&result, "@agent-malformed"));
    assert!(!has_token(&result, "$invalid-utf8"));
    assert!(!has_token(&result, "/invalid-utf8-header"));
}

#[test]
fn unterminated_and_over_budget_headers_fail_closed() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let mut unterminated = String::from("---\nname: unterminated\ndescription: Missing close\n");
    unterminated.push_str(&"x".repeat(80 * 1024));
    write(
        &cwd.join(".agents/skills/unterminated/SKILL.md"),
        &unterminated,
    );
    write(&cwd.join(".claude/commands/unterminated.md"), &unterminated);
    let mut late = String::from("---\nname: late\ndescription: Late close\npadding: ");
    late.push_str(&"x".repeat(64 * 1024));
    late.push_str("\n---\nbody");
    write(&cwd.join(".agents/skills/late/SKILL.md"), &late);
    let mut hidden_late =
        String::from("---\ndescription: Hidden late close\nhidden: true\npadding: ");
    hidden_late.push_str(&"x".repeat(64 * 1024));
    hidden_late.push_str("\n---\nbody");
    write(&cwd.join(".claude/commands/hidden-late.md"), &hidden_late);

    let result = discover(&home, &cwd);

    assert!(!has_token(&result, "$unterminated"));
    assert!(!has_token(&result, "$late"));
    assert!(!has_token(&result, "/unterminated"));
    assert!(!has_token(&result, "/hidden-late"));
}

#[test]
fn complete_file_bounds_remain_for_toml_registries_and_manifests() {
    let fixture = TempDir::new().expect("tempdir");
    let home = fixture.path().join("home");
    let cwd = fixture.path().join("repo");
    let plugin = fixture.path().join("plugins/fallback");
    write(&cwd.join(".git/HEAD"), "ref: refs/heads/main\n");
    let mut oversized_toml = String::from("name = \"oversized\"\ndescription = \"Agent\"\n");
    oversized_toml.push_str(&"# padding\n".repeat(2_000));
    write(&cwd.join(".codex/agents/oversized.toml"), &oversized_toml);
    write(
        &plugin.join("commands/default.md"),
        "---\ndescription: Default root\n---\nbody",
    );
    write(
        &plugin.join("custom/custom.md"),
        "---\ndescription: Custom root\n---\nbody",
    );
    let manifest = format!(
        "{{\"name\":\"custom-name\",\"commands\":\"custom\",\"padding\":\"{}\"}}",
        "x".repeat(65 * 1024)
    );
    write(&plugin.join(".claude-plugin/plugin.json"), &manifest);
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &format!(
            "{{\"plugins\":{{\"fallback@catalog\":[{{\"installPath\":{:?}}}]}}}}",
            plugin.to_string_lossy()
        ),
    );

    let result = discover(&home, &cwd);

    assert!(result.project.iter().all(|entry| entry.name != "oversized"));
    assert!(has_token(&result, "/fallback:default"));
    assert!(!has_token(&result, "/custom-name:custom"));

    let oversized_registry = format!(
        "{{\"plugins\":{{\"fallback@catalog\":[{{\"installPath\":{:?},\"padding\":\"{}\"}}]}}}}",
        plugin.to_string_lossy(),
        "x".repeat(512 * 1024)
    );
    write(
        &home.join(".claude/plugins/installed_plugins.json"),
        &oversized_registry,
    );
    let result = discover(&home, &cwd);
    assert!(!has_token(&result, "/fallback:default"));
}
