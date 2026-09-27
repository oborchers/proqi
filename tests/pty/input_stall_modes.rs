//! Cross-exec owner restoration and pending persistence in a real PTY.

use std::{fs, path::Path, thread, time::Duration};

use proqi::{
    adapters::sqlite::{SqliteStore, StoreConfig},
    domain::{ContentAnnotationKind, SessionId, Timestamp},
    ports::store::{MigrationMode, Store as _},
};

use super::{
    support::{consume_first_run, expect_command, json_command, wait_for_path},
    watchdog,
};

const WORKFLOW_LIMIT: Duration = Duration::from_secs(20);
const BOARD_WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    close [open $env(PROQI_TEST_PIDS) w]
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    send -- "board before"
    after 700
    send "\x1b"
    after 100
    close [open $env(PROQI_TEST_READY) w]
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    while {![file exists $env(PROQI_TEST_CONTINUE)]} { after 10 }
    send "\x1b\[A"
    send "\r"
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
const COMPOSE_WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) w]
    puts $owned [exp_pid]
    close $owned
    after 300
    close [open $env(PROQI_TEST_READY) w]
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    while {![file exists $env(PROQI_TEST_CONTINUE)]} { after 10 }
    send "\x1b\[D"
    send -- "compose after"
    after 700
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;
const PENDING_WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 15
    set content [encoding convertfrom utf-8 [binary format H* $env(PROQI_TEST_CONTENT_HEX)]]
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) w]
    puts $owned [exp_pid]
    close $owned
    send -- "\x1b\[200~"
    set offset 0
    while {$offset < [string length $content]} {
        send -- [string range $content $offset [expr {$offset + 255}]]
        after 5
        incr offset 256
    }
    send -- "\x1b\[201~"
    set waits 0
    while {![file exists $env(PROQI_TEST_BUSY)] && $waits < 1000} {
        after 10
        incr waits
    }
    if {![file exists $env(PROQI_TEST_BUSY)]} { puts "busy marker missing"; exit 94 }
    close [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    after 300
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
const CONCURRENT_WORKFLOW: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) w]
    puts $owned [exp_pid]
    close $owned
    after 300
    close [open $env(PROQI_TEST_TRIGGER) w]
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    send -- $env(PROQI_TEST_CONTENT)
    after 700
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn active_board_owner_accepts_a_board_key_after_exact_replacement() {
    run_owner_workflow(BOARD_WORKFLOW, "board before!", "mode=board");
}

#[test]
fn empty_compose_owner_accepts_a_compose_key_after_exact_replacement() {
    run_owner_workflow(COMPOSE_WORKFLOW, "compose after", "mode=compose");
}

#[test]
fn pending_unicode_commit_drains_once_before_replacement() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let pids = state.path().join("owned-pids");
    let recovered = state.path().join("recovered");
    let busy = state.path().join("persistence-busy");
    let content = "pending Grüße 界\ncontrol \u{1} and large ".to_owned() + &"x".repeat(1_500);
    let content_hex = hex(&content);
    let mut command = expect_command();
    command
        .args(["-c", PENDING_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_CONTENT_HEX", content_hex)
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_RECOVERED", &recovered)
        .env("PROQI_TEST_BUSY", &busy)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_PERSISTENCE_DELAY_MS", "1200")
        .env("PROQI_TEST_PERSISTENCE_BUSY", &busy)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        WORKFLOW_LIMIT,
        &pids,
        "pending input recovery",
    );
    assert!(status.success(), "pending recovery exited with {status}");
    assert_same_pid(&pids, &recovered);
    assert_single_thought(binary, state.path(), &(content + "!"));
    assert_large_paste_annotation(binary, state.path());
}

#[test]
fn concurrent_exact_sessions_recover_with_independent_lineage() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    thread::scope(|scope| {
        let first = scope.spawn(|| run_concurrent(state.path(), "first", "concurrent first"));
        let second = scope.spawn(|| run_concurrent(state.path(), "second", "concurrent second"));
        first.join().expect("first concurrent workflow");
        second.join().expect("second concurrent workflow");
    });
    assert_shared_runtime_contents(binary, state.path());
}

fn run_owner_workflow(script: &str, expected_content: &str, expected_mode: &str) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let pids = state.path().join("owned-pids");
    let ready = state.path().join("ready");
    let recovered = state.path().join("recovered");
    let continue_path = state.path().join("continue");
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_READY", &ready)
        .env("PROQI_TEST_RECOVERED", &recovered)
        .env("PROQI_TEST_CONTINUE", &continue_path)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_INPUT_ACCEPTANCE", "1")
        .env_remove("HERDR_ENV");
    let watchdog_pids = pids.clone();
    let workflow = thread::spawn(move || {
        watchdog::status_before(
            &mut command,
            WORKFLOW_LIMIT,
            &watchdog_pids,
            "owner recovery",
        )
    });
    wait_for_path(&ready);
    let _removed = fs::remove_file(state.path().join("runtime/input-accepted"));
    fs::write(state.path().join("runtime/input-stall-trigger"), b"stall").expect("arm input stall");
    wait_for_path(&recovered);
    fs::write(&continue_path, b"continue").expect("release post-recovery input");
    let status = workflow.join().expect("owner workflow");
    assert!(status.success(), "owner recovery exited with {status}");
    assert_same_pid(&pids, &recovered);
    assert_single_thought(binary, state.path(), expected_content);
    let acceptance = fs::read_to_string(state.path().join("runtime/input-accepted"))
        .expect("input acceptance probe");
    assert!(acceptance.contains(expected_mode), "{acceptance}");
}

fn run_concurrent(state: &Path, label: &str, content: &str) {
    let binary = env!("CARGO_BIN_EXE_proqi");
    let pids = state.join(format!("owned-pids-{label}"));
    let recovered = state.join(format!("recovered-{label}"));
    let trigger = state.join(format!("input-stall-trigger-{label}"));
    let mut command = expect_command();
    command
        .args(["-c", CONCURRENT_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_RECOVERED", &recovered)
        .env("PROQI_TEST_TRIGGER", &trigger)
        .env("PROQI_TEST_CONTENT", content)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_INPUT_STALL_TRIGGER", &trigger)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        WORKFLOW_LIMIT,
        &pids,
        "concurrent input recovery",
    );
    assert!(status.success(), "concurrent recovery exited with {status}");
    assert_same_pid(&pids, &recovered);
}

fn assert_shared_runtime_contents(binary: &str, state: &Path) {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session_ids = sessions["data"]["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .map(|session| session["id"].as_str().expect("session ID"))
        .collect::<Vec<_>>();
    assert_eq!(
        session_ids.len(),
        2,
        "shared runtime must retain both sessions"
    );
    let mut contents = session_ids
        .iter()
        .map(|session| {
            let thoughts = json_command(binary, state, &["thoughts", "list", session]);
            assert_eq!(thoughts["data"]["items"].as_array().map(Vec::len), Some(1));
            thoughts["data"]["items"][0]["content"]
                .as_str()
                .expect("thought content")
                .to_owned()
        })
        .collect::<Vec<_>>();
    contents.sort();
    assert_eq!(contents, ["concurrent first", "concurrent second"]);
}

fn assert_same_pid(pids: &std::path::Path, recovered: &std::path::Path) {
    let before = fs::read_to_string(pids).expect("initial PID");
    let after = fs::read_to_string(recovered).expect("recovered PID");
    assert_eq!(before.trim(), after.trim(), "Unix exec must preserve PID");
}

fn assert_single_thought(binary: &str, state: &std::path::Path, expected: &str) {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state, &["thoughts", "list", session]);
    assert_eq!(thoughts["data"]["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(thoughts["data"]["items"][0]["content"], expected);
}

fn assert_large_paste_annotation(binary: &str, state: &Path) {
    let sessions = json_command(binary, state, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID")
        .parse::<SessionId>()
        .expect("typed session ID");
    let config = StoreConfig::new(
        state.join("data/proqi.sqlite3"),
        state.join("data/backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    let mut store = SqliteStore::open(&config).expect("open recovered store");
    let snapshot = store.load_session(session).expect("load recovered session");
    let thought = snapshot.board.live_thoughts()[0];
    assert!(thought.annotations.iter().any(|annotation| matches!(
        annotation.kind,
        ContentAnnotationKind::LargePaste { graphemes, .. } if graphemes >= 1_200
    )));
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
