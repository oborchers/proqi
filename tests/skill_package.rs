//! Repository skill packaging and stable CLI discovery contracts.

use std::{process::Command, str::FromStr};

use proqi::domain::{OperationId, SessionId, ThoughtId};

const SKILL: &str = include_str!("../skills/proqi/SKILL.md");
const OPENAI: &str = include_str!("../skills/proqi/agents/openai.yaml");

#[test]
fn skill_is_explicit_json_only_and_example_identifiers_are_canonical() {
    assert!(SKILL.contains("Act only after explicit invocation."));
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
fn every_documented_command_family_remains_available() {
    for arguments in [
        &["capabilities", "--help"][..],
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
