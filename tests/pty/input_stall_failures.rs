//! Failed input-stall replacement preserves exact resumability without a loop.

use std::{fs, os::unix::fs::PermissionsExt as _, path::Path, thread, time::Duration};

use super::{
    support::{consume_first_run, expect_command, json_command, wait_for_path},
    watchdog,
};

const WORKFLOW_LIMIT: Duration = Duration::from_secs(20);
const WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    send -- "replacement failure durable"
    after 700
    set ready [open $env(PROQI_TEST_READY) w]
    puts $ready ready
    close $ready
    while {![file exists $env(PROQI_TEST_CONTINUE)]} { after 10 }
    set trigger [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    puts $trigger stall
    close $trigger
    expect -exact "\x1b\[?1049l"
    expect -glob "*$env(PROQI_TEST_EXPECTED)*exact resume command:*"
    expect eof
    catch wait result
    if {[lindex $result 3] == 0} { exit 93 }
    exit 0
"#;
const PERSISTENCE_FAILURE_WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 15
    set content [encoding convertfrom utf-8 [binary format H* $env(PROQI_TEST_CONTENT_HEX)]]
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) w]
    puts $owned [exp_pid]
    close $owned
    send -- "\x1b\[200~$content\x1b\[201~"
    while {![file exists $env(PROQI_TEST_FAILED)]} { after 10 }
    after 300
    close [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    expect -exact "\x1b\[?1049l"
    expect -glob "*recovery preparation failed:*exact resume command:*optimistic recovery file:*"
    expect eof
    catch wait result
    if {[lindex $result 3] == 0} { exit 92 }
    exit 0
"#;
const PERSISTENCE_RETRY_WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 15
    set content [encoding convertfrom utf-8 [binary format H* $env(PROQI_TEST_CONTENT_HEX)]]
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) w]
    puts $owned [exp_pid]
    close $owned
    send -- "\x1b\[200~$content\x1b\[201~"
    while {![file exists $env(PROQI_TEST_FAILED)]} { after 10 }
    after 300
    send -- "r"
    after 700
    close [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    send -- "!"
    after 700
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[derive(Clone, Copy)]
enum Mutation {
    ChangeIdentity,
    Remove,
    RemoveExecutePermission,
}

#[test]
fn changed_executable_identity_fails_closed_before_exec() {
    run_failure(Mutation::ChangeIdentity, "identity changed");
}

#[test]
fn unavailable_executable_fails_closed_before_exec() {
    run_failure(Mutation::Remove, "became unavailable");
}

#[test]
fn failed_exec_returns_exact_resume_guidance_without_retry() {
    run_failure(
        Mutation::RemoveExecutePermission,
        "process replacement failed",
    );
}

#[test]
fn failed_persistence_exports_optimistic_state_before_bounded_exit() {
    let content = "unsaved persistence Grüße 界 control \u{1}";
    let state = run_persistence_workflow(PERSISTENCE_FAILURE_WORKFLOW, content, false, true);
    let recovery = fs::read_dir(state.path().join("runtime/recovery-fallback"))
        .expect("recovery directory")
        .map(|entry| entry.expect("recovery entry").path())
        .collect::<Vec<_>>();
    assert_eq!(recovery.len(), 1, "one exact recovery export is expected");
    let metadata = fs::metadata(&recovery[0]).expect("recovery metadata");
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    let document: serde_json::Value =
        serde_json::from_slice(&fs::read(&recovery[0]).expect("recovery document"))
            .expect("recovery JSON");
    assert_eq!(document["thoughts"][0]["content"], content);
    assert!(
        document["failed_sequence"].as_u64().is_some(),
        "export must identify the failed sequence"
    );
    assert!(
        document["pending_sequences"]
            .as_array()
            .is_some_and(|sequences| !sequences.is_empty()),
        "export must retain pending durability ownership"
    );
    assert_eq!(
        fs::read(state.path().join("unrelated-recovery-target")).expect("unrelated target"),
        b"unrelated"
    );
    assert_single_content(state.path(), None);
}

#[test]
fn retained_persistence_retry_can_recover_and_then_replace_exactly() {
    let content = "retry persistence Grüße 界";
    let state = run_persistence_workflow(PERSISTENCE_RETRY_WORKFLOW, content, true, false);
    assert_single_content(state.path(), Some(&(content.to_owned() + "!")));
}

fn run_failure(mutation: Mutation, expected: &str) {
    let temporary = tempfile::tempdir().expect("temporary harness");
    let state = temporary.path().join("state");
    let binary = temporary.path().join("proqi-under-test");
    fs::copy(env!("CARGO_BIN_EXE_proqi"), &binary).expect("copy topic binary");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
        .expect("executable permissions");
    let binary_text = binary.to_str().expect("UTF-8 binary path");
    consume_first_run(binary_text, &state);
    let pids = temporary.path().join("owned-pids");
    let ready = temporary.path().join("ready");
    let continue_path = temporary.path().join("continue");
    let mut command = expect_command();
    command
        .args(["-c", WORKFLOW])
        .env("PROQI_TEST_BINARY", &binary)
        .env("PROQI_TEST_STATE", &state)
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_READY", &ready)
        .env("PROQI_TEST_CONTINUE", &continue_path)
        .env("PROQI_TEST_EXPECTED", expected)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env_remove("HERDR_ENV");
    let workflow = thread::spawn(move || {
        watchdog::status_before(
            &mut command,
            WORKFLOW_LIMIT,
            &pids,
            "failed input replacement",
        )
    });

    wait_for_path(&ready);
    mutate_executable(&binary, mutation);
    fs::write(&continue_path, b"continue").expect("release failure workflow");
    let status = workflow.join().expect("PTY watcher");
    assert!(
        status.success(),
        "failed replacement workflow exited with {status}"
    );

    let sessions = json_command(env!("CARGO_BIN_EXE_proqi"), &state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(
        env!("CARGO_BIN_EXE_proqi"),
        &state,
        &["thoughts", "list", session],
    );
    assert_eq!(
        thoughts["data"]["items"][0]["content"],
        "replacement failure durable"
    );
}

fn run_persistence_workflow(
    workflow: &str,
    content: &str,
    expects_replacement: bool,
    corrupt_primary_recovery: bool,
) -> tempfile::TempDir {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    if corrupt_primary_recovery {
        let target = state.path().join("unrelated-recovery-target");
        fs::write(&target, b"unrelated").expect("recovery symlink target");
        std::os::unix::fs::symlink(&target, state.path().join("data/recovery"))
            .expect("corrupt primary recovery directory");
    }
    let pids = state.path().join("owned-pids");
    let failed = state.path().join("persistence-failed");
    let recovered = state.path().join("recovered");
    let mut command = expect_command();
    command
        .args(["-c", workflow])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_CONTENT_HEX", hex(content))
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_FAILED", &failed)
        .env("PROQI_TEST_RECOVERED", &recovered)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_PERSISTENCE_FAIL_ONCE", "1")
        .env("PROQI_TEST_PERSISTENCE_FAILED", &failed)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        WORKFLOW_LIMIT,
        &pids,
        "persistence failure input recovery",
    );
    assert!(
        status.success(),
        "persistence workflow exited with {status}"
    );
    assert_eq!(recovered.exists(), expects_replacement);
    if expects_replacement {
        let before = fs::read_to_string(&pids).expect("initial PID");
        let after = fs::read_to_string(&recovered).expect("recovered PID");
        assert_eq!(before.trim(), after.trim());
    }
    state
}

fn assert_single_content(state: &Path, expected: Option<&str>) {
    let binary = env!("CARGO_BIN_EXE_proqi");
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state, &["thoughts", "list", session]);
    let contents = thoughts["data"]["items"]
        .as_array()
        .expect("thoughts")
        .iter()
        .map(|thought| thought["content"].as_str().expect("thought content"))
        .collect::<Vec<_>>();
    assert_eq!(contents, expected.into_iter().collect::<Vec<_>>());
}

fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .fold(String::new(), |mut out, byte| {
            use std::fmt::Write as _;
            write!(&mut out, "{byte:02x}").expect("hex byte");
            out
        })
}

fn mutate_executable(binary: &Path, mutation: Mutation) {
    match mutation {
        Mutation::ChangeIdentity => {
            let replacement = binary.with_extension("replacement");
            fs::copy(binary, &replacement).expect("replacement copy");
            let mut bytes = fs::read(&replacement).expect("replacement bytes");
            bytes.push(0);
            fs::write(&replacement, bytes).expect("changed replacement bytes");
            fs::set_permissions(&replacement, fs::Permissions::from_mode(0o755))
                .expect("replacement permissions");
            fs::rename(replacement, binary).expect("replace executable identity");
        }
        Mutation::Remove => {
            fs::rename(binary, binary.with_extension("moved")).expect("move executable");
        }
        Mutation::RemoveExecutePermission => {
            fs::set_permissions(binary, fs::Permissions::from_mode(0o600))
                .expect("remove execute permission");
        }
    }
}
