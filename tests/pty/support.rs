//! Shared process-boundary primitives for the PTY integration suite.
//!
//! This module owns Expect construction, bounded owner-readiness polling, and
//! JSON CLI invocation. Product scenarios and their behavioral assertions
//! belong in sibling modules.

use std::process::Command;

use serde_json::Value;

pub(super) fn expect_command() -> Command {
    let mut command = Command::new("/usr/bin/expect");
    command.env("PROQI_DISABLE_HERDR", "1");
    command
}

pub(super) fn wait_for_path(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

pub(super) fn wait_for_control_owner(state: &std::path::Path, session: &str) {
    let instances = state.join("runtime/instances");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if control_owner_is_ready(&instances, session) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not advertise a ready control endpoint"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn control_owner_is_ready(instances: &std::path::Path, session: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(instances) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|entry| control_metadata_is_ready(&entry.path(), session))
}

fn control_metadata_is_ready(path: &std::path::Path, session: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    value["session_id"] == session
        && value["control_protocol"].as_u64()
            == Some(u64::from(proqi::ports::control::CONTROL_PROTOCOL_VERSION))
        && value["control_endpoint"]
            .as_str()
            .is_some_and(|endpoint| std::path::Path::new(endpoint).exists())
}

pub(super) fn raw_input_command(
    binary: &str,
    state: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> std::process::Output {
    use std::{io::Write as _, process::Stdio};

    let mut child = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn input command");
    child
        .stdin
        .take()
        .expect("command stdin")
        .write_all(input.as_bytes())
        .expect("write command input");
    child.wait_with_output().expect("wait for input command")
}

pub(super) fn json_input_command(
    binary: &str,
    state: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> Value {
    let output = raw_input_command(binary, state, arguments, input);
    assert!(
        output.status.success(),
        "input command failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("input command JSON")
}

pub(super) fn json_command(binary: &str, state: &std::path::Path, arguments: &[&str]) -> Value {
    let output = Command::new(binary)
        .arg("--state-dir")
        .arg(state)
        .arg("--json")
        .args(arguments)
        .output()
        .expect("run JSON command");
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("JSON output")
}
