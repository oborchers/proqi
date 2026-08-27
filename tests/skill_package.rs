//! Repository skill packaging and stable CLI discovery contracts.

use std::{fs, path::Path, process::Command, str::FromStr};

use proqi::domain::{OperationId, SessionId, ThoughtId};

const SKILL: &str = include_str!("../skills/proqi/SKILL.md");
const OPENAI: &str = include_str!("../skills/proqi/agents/openai.yaml");
const DEBUG_SKILL: &str = include_str!("../skills/proqi-debug/SKILL.md");
const DEBUG_OPENAI: &str = include_str!("../skills/proqi-debug/agents/openai.yaml");
const DEBUG_STORAGE: &str = include_str!("../skills/proqi-debug/references/storage.md");
const RELEASE_SKILL: &str = include_str!("../.agents/skills/release/SKILL.md");
const RELEASE_NOTES: &str = include_str!("../.agents/skills/release/references/release-notes.md");
const README: &str = include_str!("../README.md");

#[test]
fn skill_is_explicit_json_only_and_example_identifiers_are_canonical() {
    assert!(SKILL.contains("Act only after explicit invocation."));
    assert!(SKILL.contains("installed JSON CLI is the only application boundary"));
    assert!(OPENAI.contains("allow_implicit_invocation: false"));
    assert!(
        SKILL
            .lines()
            .filter(|line| line.starts_with("proqi "))
            .all(|line| line.starts_with("proqi --json "))
    );

    SessionId::from_str("ses_06g30t7dv5qv55n1ppn3clis3k").expect("canonical session fixture");
    ThoughtId::from_str("tht_06g30t8fudrq55fdkk348i7388").expect("canonical thought fixture");
    OperationId::from_str("op_06g30t8fudrq55fdkjqr6mpe44").expect("canonical operation fixture");
}

#[test]
fn canonical_skills_cli_installation_is_documented_as_a_separate_step() {
    assert!(README.contains("npx skills add oborchers/proqi --skill proqi -g"));
    assert!(README.contains("--agent codex --agent claude-code"));
    assert!(README.contains("The skill does not install the Proqi executable."));
    assert!(SKILL.starts_with("---\nname: proqi\ndescription:"));
    assert!(OPENAI.contains("allow_implicit_invocation: false"));
}

#[test]
fn debug_skill_is_read_only_first_and_issue_creation_requires_approval() {
    assert!(DEBUG_SKILL.starts_with("---\nname: proqi-debug\ndescription:"));
    assert!(DEBUG_SKILL.contains("proqi diagnostics collect --output"));
    assert!(DEBUG_SKILL.contains("proqi --json doctor"));
    assert!(DEBUG_SKILL.contains("Do not mutate SQLite"));
    assert!(DEBUG_SKILL.contains("explicit approval"));
    assert!(DEBUG_SKILL.contains("gh issue create"));
    assert!(DEBUG_SKILL.contains("SECURITY.md"));
    assert!(DEBUG_OPENAI.contains("$proqi-debug"));
    assert!(DEBUG_OPENAI.contains("allow_implicit_invocation: false"));
    assert!(DEBUG_STORAGE.contains("submission_attempts"));
    assert!(DEBUG_STORAGE.contains("`-wal` and"));
    assert!(DEBUG_STORAGE.contains("copied independently"));
    assert!(README.contains("npx skills add oborchers/proqi --skill proqi-debug -g"));
}

#[test]
fn local_release_skill_requires_exact_publication_confirmation() {
    assert!(RELEASE_SKILL.starts_with("---\nname: release\ndescription:"));
    assert!(RELEASE_SKILL.contains("This is a repository-local maintainer skill."));
    assert!(RELEASE_SKILL.contains("Always stop immediately before creating or pushing"));
    assert!(RELEASE_SKILL.contains("This confirmation is mandatory"));
    assert!(RELEASE_SKILL.contains("Do not infer it from earlier authority."));
    assert!(RELEASE_SKILL.contains("Do not create or publish a GitHub\n   Release manually."));
    assert!(RELEASE_NOTES.contains(".github/release-notes/vX.Y.Z.md"));
    assert!(RELEASE_NOTES.contains("## Review checklist"));

    let claude_skill = Path::new(env!("CARGO_MANIFEST_DIR")).join(".claude/skills/release");
    assert!(
        fs::symlink_metadata(&claude_skill)
            .expect("Claude release skill metadata")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(claude_skill).expect("Claude release skill target"),
        Path::new("../../.agents/skills/release")
    );
}

#[test]
fn every_documented_command_family_remains_available() {
    for arguments in [
        &["capabilities", "--help"][..],
        &["doctor", "--help"],
        &["sessions", "list", "--help"],
        &["thoughts", "list", "--help"],
        &["thoughts", "inspect", "--help"],
        &["thoughts", "add", "--help"],
        &["thoughts", "move", "--help"],
        &["thoughts", "send", "--help"],
        &["thoughts", "delete", "--help"],
        &["thoughts", "undo", "--help"],
        &["thoughts", "redo", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
            .args(arguments)
            .output()
            .expect("run documented command help");
        assert!(
            output.status.success(),
            "documented command failed: {}",
            arguments.join(" ")
        );
    }
}
