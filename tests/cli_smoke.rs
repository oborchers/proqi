//! Command-line smoke tests for the installed binary contract.

use std::process::Command;

#[test]
fn help_and_version_are_available() {
    let binary = env!("CARGO_BIN_EXE_proqi");

    let help = Command::new(binary)
        .arg("--help")
        .output()
        .expect("run proqi --help");
    assert!(help.status.success());
    assert!(
        String::from_utf8_lossy(&help.stdout)
            .contains("agent-optimized scratchpad for follow-up prompts")
    );

    let version = Command::new(binary)
        .arg("--version")
        .output()
        .expect("run proqi --version");
    assert!(version.status.success());
    assert_eq!(String::from_utf8_lossy(&version.stdout), "proqi 0.1.1\n");

    for shell in ["bash", "zsh", "fish"] {
        let completion = Command::new(binary)
            .args(["completions", shell])
            .output()
            .expect("generate completions");
        assert!(completion.status.success(), "{shell} completion failed");
        assert!(completion.stdout.len() > 100, "{shell} completion is empty");
        assert!(String::from_utf8_lossy(&completion.stdout).contains("proqi"));
    }
}
