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
    assert!(String::from_utf8_lossy(&help.stdout).contains("terminal-native scratchpad"));

    let version = Command::new(binary)
        .arg("--version")
        .output()
        .expect("run proqi --version");
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("proqi "));
}
