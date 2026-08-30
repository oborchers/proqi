//! Shared real-process and pseudo-terminal fixture helpers.

use std::{
    io::Write as _,
    path::Path,
    process::{Command, Output, Stdio},
};

use serde_json::Value;

pub(super) fn expect_command() -> Command {
    let mut command = Command::new("/usr/bin/expect");
    command.env("PROQI_DISABLE_HERDR", "1");
    command
}

pub(super) fn wait_for_path(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

pub(super) fn wait_for_control_owner(state: &Path, session: &str) {
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

fn control_owner_is_ready(instances: &Path, session: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(instances) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|entry| control_metadata_is_ready(&entry.path(), session))
}

fn control_metadata_is_ready(path: &Path, session: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    value["session_id"] == session
        && value["control_protocol"].as_u64() == Some(5)
        && value["control_endpoint"]
            .as_str()
            .is_some_and(|endpoint| Path::new(endpoint).exists())
}

pub(super) fn raw_input_command(
    binary: &str,
    state: &Path,
    arguments: &[&str],
    input: &str,
) -> Output {
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
    state: &Path,
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

pub(super) fn json_command(binary: &str, state: &Path, arguments: &[&str]) -> Value {
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

pub(super) fn consume_first_run(binary: &str, state: &Path) {
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        after 400
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env_remove("HERDR_ENV")
        .status()
        .expect("consume first-run practice board");
    assert!(status.success());
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let values = sessions["data"]["sessions"]
        .as_array()
        .expect("practice session");
    assert_eq!(values.len(), 1);
    let session = values[0]["id"].as_str().expect("practice session ID");
    let _trashed = json_command(binary, state, &["sessions", "trash", session]);
    let _pruned = json_command(binary, state, &["sessions", "prune", session, "--yes"]);
    assert!(
        json_command(binary, state, &["sessions", "list"])["data"]["sessions"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
}
