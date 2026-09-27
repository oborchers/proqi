//! Real terminal title editing, mouse entry, resize, and persistence recovery.

use super::support::{consume_first_run, expect_command, json_command, json_input_command};

const NAME_WORKFLOW: &str = r#"
    log_user 0
    set timeout 12
    spawn -noecho sh -c {stty rows 12 columns 48; exec "$PROQI_TEST_BINARY" --state-dir "$PROQI_TEST_STATE" -r "$PROQI_TEST_SESSION"}
    expect -exact "Mouse"
    send -- "\x1b\[<0;4;1M\x1b\[<0;4;1m"
    after 100
    send -- $env(PROQI_TEST_PRIMARY_A)
    send -- "\x1b\[200~  Mouse\r\nretry  \x1b\[201~"
    send -- "\r"
    set waits 0
    while {![file exists $env(PROQI_TEST_PERSISTENCE_BUSY)] && $waits < 300} {
        after 10
        incr waits
    }
    if {![file exists $env(PROQI_TEST_PERSISTENCE_BUSY)]} { exit 91 }
    set waits 0
    while {![file exists $env(PROQI_TEST_PERSISTENCE_FAILED)] && $waits < 300} {
        after 10
        incr waits
    }
    if {![file exists $env(PROQI_TEST_PERSISTENCE_FAILED)]} { exit 92 }
    expect -re "storage is busy"
    send -- "r"
    expect -re "saved"
    stty rows 4 columns 12
    after 100
    stty rows 30 columns 100
    after 100
    send -- "\x12"
    send -- $env(PROQI_TEST_PRIMARY_A)
    send -- "\x1b\[200~cancelled title\x1b\[201~"
    send -- "\x1b"
    after 100
    send -- "q"
    expect {
        eof {}
        timeout { exit 93 }
    }
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn mouse_rename_failure_retry_cancel_resize_and_restart_preserve_exact_payloads() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let created = json_command(binary, state.path(), &[]);
    let session = created["data"]["session_id"].as_str().expect("session ID");
    let body = "Mouse body remains exact\nGrüße 第二行 👩‍💻";
    let added = json_input_command(binary, state.path(), &["thoughts", "add", session], body);
    let thought = added["data"]["thought_id"].as_str().expect("thought ID");
    let _named = json_command(
        binary,
        state.path(),
        &["thoughts", "rename", session, thought, "Mouse"],
    );
    let pending = state.path().join("name-pending");
    let failed = state.path().join("name-failed");
    let status = expect_command()
        .args(["-c", NAME_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_PERSISTENCE_DELAY_MS", "800")
        .env("PROQI_TEST_PERSISTENCE_BUSY", &pending)
        .env("PROQI_TEST_PERSISTENCE_FAIL_ONCE", "1")
        .env("PROQI_TEST_PERSISTENCE_FAILED", &failed)
        .env_remove("HERDR_ENV")
        .status()
        .expect("run thought-name PTY workflow");
    assert!(status.success(), "thought-name PTY exited with {status}");
    assert!(pending.exists(), "pending persistence was not observed");
    assert!(
        failed.exists(),
        "injected persistence failure was not observed"
    );

    let inspected = json_command(
        binary,
        state.path(),
        &["thoughts", "inspect", session, thought],
    );
    assert_eq!(inspected["data"]["thought"]["name"], "Mouse retry");
    assert_eq!(inspected["data"]["thought"]["content"], body);
}
