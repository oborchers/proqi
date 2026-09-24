//! Repository skill packaging and stable CLI discovery contracts.

use std::{collections::BTreeSet, fs, path::Path, process::Command, str::FromStr};

use proqi::domain::{OperationId, SeparatorId, SessionId, ThoughtId};
use serde_json::Value;

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
    SeparatorId::from_str("sep_06g30t8fudrq55fdkjqr6mpe44").expect("canonical separator fixture");
    OperationId::from_str("op_06g30t8fudrq55fdkjqr6mpe44").expect("canonical operation fixture");
}

#[test]
fn bundled_operation_inventory_matches_the_real_capabilities_contract() {
    let documented = documented_operations();

    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args(["--json", "capabilities"])
        .output()
        .expect("run capabilities");
    assert!(output.status.success());
    let live: Value = serde_json::from_slice(&output.stdout).expect("capability response");
    assert_eq!(documented, live["data"]["operations"]);
}

#[test]
fn skill_references_the_documented_error_contract() {
    const CLI_REFERENCE: &str = include_str!("../docs/reference/cli.md");
    assert!(SKILL.contains("https://oborchers.github.io/proqi/reference/cli.html#errors"));
    assert!(SKILL.contains("data.error_codes"));
    assert!(CLI_REFERENCE.contains("\n## Errors\n"));
    assert!(!SKILL.contains("legacy thought projection"));
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
fn debug_skill_commands_are_advertised_and_callable() {
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args(["--json", "capabilities"])
        .output()
        .expect("run capabilities");
    assert!(output.status.success());
    let capabilities: Value = serde_json::from_slice(&output.stdout).expect("capability response");
    assert!(
        capabilities["data"]["commands"]
            .as_array()
            .expect("commands")
            .iter()
            .any(|command| command == "doctor")
    );
    assert!(
        capabilities["data"]["operations"]["diagnostics"]
            .as_array()
            .expect("diagnostic operations")
            .iter()
            .any(|operation| operation == "collect")
    );

    for arguments in [&["doctor"][..], &["diagnostics", "collect"][..]] {
        let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
            .args(arguments)
            .arg("--help")
            .output()
            .expect("run debug command help");
        assert!(output.status.success());
    }
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
    let operations = documented_operations();
    for family in ["diagnostics", "sessions", "items", "thoughts", "update"] {
        let documented = operations[family]
            .as_array()
            .expect("operation array")
            .iter()
            .map(|operation| operation.as_str().expect("operation name").to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(documented, help_subcommands(family));
        for operation in operations[family].as_array().expect("operation array") {
            let operation = operation.as_str().expect("operation name");
            let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
                .args([family, operation, "--help"])
                .output()
                .expect("run documented command help");
            assert!(
                output.status.success(),
                "documented command failed: {family} {operation}"
            );
        }
    }

    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--help")
        .output()
        .expect("run top-level help");
    assert!(output.status.success());
    let actual = parse_subcommands(&String::from_utf8(output.stdout).expect("UTF-8 help"));
    let capabilities = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args(["--json", "capabilities"])
        .output()
        .expect("run capabilities");
    let capabilities: Value =
        serde_json::from_slice(&capabilities.stdout).expect("capability response");
    let advertised = capabilities["data"]["commands"]
        .as_array()
        .expect("command array")
        .iter()
        .map(|command| command.as_str().expect("command name").to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(advertised, actual);
}

fn help_subcommands(family: &str) -> BTreeSet<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .args([family, "--help"])
        .output()
        .expect("run command-family help");
    assert!(output.status.success());
    parse_subcommands(&String::from_utf8(output.stdout).expect("UTF-8 help"))
}

fn parse_subcommands(help: &str) -> BTreeSet<String> {
    help.lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|command| *command != "help")
        .map(str::to_owned)
        .collect()
}

fn documented_operations() -> Value {
    let marker = "## Capability inventory";
    let section = SKILL.split_once(marker).expect("capability section").1;
    let fenced = section.split_once("```json\n").expect("JSON fence").1;
    let documented = fenced.split_once("\n```").expect("closed JSON fence").0;
    serde_json::from_str(documented).expect("capability JSON")
}
