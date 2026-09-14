//! Exact-session automatic continuity after a confirmed input-reader stall.

use std::{fs, thread, time::Duration};

use super::{
    support::{consume_first_run, expect_command, json_command, wait_for_path},
    watchdog,
};
use proqi::{
    adapters::runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
    ports::{
        environment::{Clock as _, IdGenerator as _},
        runtime::RuntimeCoordinator as _,
    },
};

const CONTENT: &str = "stall boundary Grüße 界\ncontrol  retained";
const WORKFLOW_LIMIT: Duration = Duration::from_secs(20);
const WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 15
    set content [encoding convertfrom utf-8 [binary format H* $env(PROQI_TEST_CONTENT_HEX)]]
    proc register_pid {pid} {
        global env
        set owned [open $env(PROQI_TEST_PIDS) a]
        puts $owned $pid
        close $owned
    }
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    register_pid [exp_pid]
    send -- "\x1b\[200~$content\x1b\[201~"
    after 800
    send -- $env(PROQI_TEST_PRIMARY_A)
    after 100
    set ready [open $env(PROQI_TEST_READY) w]
    puts $ready ready
    close $ready
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    while {![file exists $env(PROQI_TEST_CONTINUE)]} { after 10 }
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
const DEFAULT_ROOT_WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    send -- "default root continuity"
    after 700
    set ready [open $env(PROQI_TEST_READY) w]
    puts $ready ready
    close $ready
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    set recovered [open $env(PROQI_TEST_RECOVERED) w]
    puts $recovered [exp_pid]
    close $recovered
    while {![file exists $env(PROQI_TEST_CONTINUE)]} { after 10 }
    send -- "!"
    after 600
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;
const PROBATION_FAILURE: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    send -- "probation durable"
    after 700
    set trigger [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    puts $trigger stall
    close $trigger
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    expect -exact "\x1b\[?1049l"
    expect -glob "*exact resume command:*"
    expect eof
    catch wait result
    if {[lindex $result 3] == 0} { exit 91 }
    exit 0
"#;
const CIRCUIT_BREAKER: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    send -- "breaker durable"
    after 700
    for {set incident 1} {$incident <= 3} {incr incident} {
        if {$incident == 3} {
            file delete -force $env(PROQI_TEST_BUSY)
            send -- "!"
            while {![file exists $env(PROQI_TEST_BUSY)]} { after 10 }
        }
        set trigger [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
        puts $trigger stall
        close $trigger
        expect -exact "\x1b\[?1049l"
        if {$incident < 3} {
            expect -exact "\x1b\[?1049h"
            after 300
        }
    }
    expect -glob "*circuit_open*exact resume command:*"
    expect eof
    catch wait result
    if {[lindex $result 3] == 0} { exit 92 }
    exit 0
"#;
const EXPIRED_INCIDENT: &str = r#"
    log_user 0
    set timeout 15
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) a]
    puts $owned [exp_pid]
    close $owned
    send -- "expired durable"
    after 700
    set trigger [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    puts $trigger stall
    close $trigger
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    after 1200
    set trigger [open "$env(PROQI_TEST_STATE)/runtime/input-stall-trigger" w]
    puts $trigger stall
    close $trigger
    expect -exact "\x1b\[?1049l"
    expect -exact "\x1b\[?1049h"
    after 300
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn viable_pty_replaces_exact_session_and_accepts_input_after_recovery() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let pids = state.path().join("owned-pids");
    let ready = state.path().join("ready-for-stall");
    let recovered = state.path().join("recovered");
    let continue_path = state.path().join("continue-after-proof");
    let content_hex = CONTENT
        .as_bytes()
        .iter()
        .fold(String::new(), |mut out, byte| {
            use std::fmt::Write as _;
            write!(&mut out, "{byte:02x}").expect("hex byte");
            out
        });
    let mut command = expect_command();
    command
        .args(["-c", WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_CONTENT_HEX", content_hex)
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_READY", &ready)
        .env("PROQI_TEST_RECOVERED", &recovered)
        .env("PROQI_TEST_CONTINUE", &continue_path)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_INPUT_ACCEPTANCE", "1")
        .env_remove("HERDR_ENV");
    let workflow = thread::spawn(move || {
        watchdog::status_before(&mut command, WORKFLOW_LIMIT, &pids, "input recovery PTY")
    });

    wait_for_path(&ready);
    let before = wait_for_any_owner(state.path());
    let session = before.session_id.to_string();
    fs::write(state.path().join("runtime/input-stall-trigger"), b"stall")
        .expect("arm one reader stall");
    wait_for_path(&recovered);
    let after = wait_for_new_owner(state.path(), &session, before.instance_id);
    assert_eq!(after.session_id, before.session_id);
    assert_eq!(after.pid, before.pid, "Unix exec must preserve the PID");
    assert_ne!(after.instance_id, before.instance_id);
    assert_eq!(after.launch_directory, before.launch_directory);
    fs::write(&continue_path, b"continue").expect("release post-recovery input");

    let status = workflow.join().expect("PTY watcher");
    assert!(status.success(), "input recovery PTY exited with {status}");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", &session]);
    assert_eq!(thoughts["data"]["thoughts"][0]["content"], "!");
    let acceptance = fs::read_to_string(state.path().join("runtime/input-accepted"))
        .expect("input acceptance probe");
    assert!(
        acceptance.lines().any(|line| line.contains("mode=edit")),
        "a real post-recovery key must reach the restored Edit owner: {acceptance}"
    );
}

#[test]
fn default_state_root_is_preserved_across_exact_replacement() {
    let home = tempfile::tempdir().expect("temporary home");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_default_first_run(binary, home.path());
    let runtime = home
        .path()
        .join("Library/Application Support/proqi/runtime");
    let pids = home.path().join("owned-pids");
    let ready = home.path().join("ready-for-stall");
    let recovered = home.path().join("recovered");
    let continue_path = home.path().join("continue-after-proof");
    let mut command = expect_command();
    command
        .args(["-c", DEFAULT_ROOT_WORKFLOW])
        .env("HOME", home.path())
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_READY", &ready)
        .env("PROQI_TEST_RECOVERED", &recovered)
        .env("PROQI_TEST_CONTINUE", &continue_path)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_INPUT_ACCEPTANCE", "1")
        .env_remove("HERDR_ENV");
    let workflow = thread::spawn(move || {
        watchdog::status_before(&mut command, WORKFLOW_LIMIT, &pids, "default-root recovery")
    });
    wait_for_path(&ready);
    let before = wait_for_owner_in(&runtime);
    fs::write(runtime.join("input-stall-trigger"), b"stall").expect("arm reader stall");
    wait_for_path(&recovered);
    let after = wait_for_new_owner_in(&runtime, before.session_id, before.instance_id);
    assert_eq!(after.session_id, before.session_id);
    assert_eq!(after.pid, before.pid);
    fs::write(&continue_path, b"continue").expect("release post-recovery input");
    let status = workflow.join().expect("PTY watcher");
    assert!(
        status.success(),
        "default-root workflow exited with {status}"
    );
    let thoughts = json_default(
        binary,
        home.path(),
        &["thoughts", "list", &before.session_id.to_string()],
    );
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        "default root continuity!"
    );
}

#[test]
fn probation_stall_exits_once_with_exact_resume_guidance() {
    run_bounded_failure(PROBATION_FAILURE, true, None);
}

#[test]
fn third_incident_inside_window_trips_the_per_session_circuit() {
    run_bounded_failure(CIRCUIT_BREAKER, false, None);
}

#[test]
fn independent_incident_after_expiry_recovers_again() {
    run_bounded_failure(EXPIRED_INCIDENT, false, Some("1000"));
}

fn run_bounded_failure(script: &str, probation: bool, window: Option<&str>) {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let pids = state.path().join("owned-pids");
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_PIDS", &pids)
        .env("PROQI_TEST_INPUT_STALL", "1")
        .env("PROQI_TEST_INPUT_ACCEPTANCE", "1")
        .env_remove("HERDR_ENV");
    if probation {
        command.env("PROQI_TEST_INPUT_STALL_PROBATION", "1");
    }
    if let Some(window) = window {
        command.env("PROQI_TEST_INPUT_RECOVERY_WINDOW_MS", window);
    }
    let busy = state.path().join("persistence-busy");
    if window.is_none() && !probation {
        command
            .env("PROQI_TEST_PERSISTENCE_DELAY_MS", "1200")
            .env("PROQI_TEST_PERSISTENCE_BUSY", &busy)
            .env("PROQI_TEST_BUSY", &busy);
    }
    let status = watchdog::status_before(&mut command, WORKFLOW_LIMIT, &pids, "bounded recovery");
    assert!(
        status.success(),
        "bounded recovery workflow exited with {status}"
    );
    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    assert_eq!(
        thoughts["data"]["thoughts"][0]["content"],
        match window {
            Some(_) => "expired durable",
            None if probation => "probation durable",
            None => "breaker durable!",
        }
    );
}

fn wait_for_any_owner(state: &std::path::Path) -> proqi::ports::runtime::InstanceInfo {
    wait_for_owner_in(&state.join("runtime"))
}

fn wait_for_owner_in(runtime: &std::path::Path) -> proqi::ports::runtime::InstanceInfo {
    let mut ids = SystemIdGenerator;
    let observer = FileRuntimeCoordinator::new(
        runtime.to_path_buf(),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("runtime observer");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(owner) = observer
            .active_instances()
            .expect("scan active owners")
            .into_iter()
            .next()
        {
            return owner;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "active owner did not appear"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_new_owner(
    state: &std::path::Path,
    session: &str,
    previous: proqi::domain::InstanceId,
) -> proqi::ports::runtime::InstanceInfo {
    wait_for_new_owner_in(
        &state.join("runtime"),
        session.parse().expect("session ID"),
        previous,
    )
}

fn wait_for_new_owner_in(
    runtime: &std::path::Path,
    session: proqi::domain::SessionId,
    previous: proqi::domain::InstanceId,
) -> proqi::ports::runtime::InstanceInfo {
    let mut ids = SystemIdGenerator;
    let observer = FileRuntimeCoordinator::new(
        runtime.to_path_buf(),
        ids.instance_id(),
        std::env::current_dir().expect("current directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("runtime observer");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(owner) = observer
            .active_instances()
            .expect("scan replacement owners")
            .into_iter()
            .find(|owner| owner.session_id == session && owner.instance_id != previous)
        {
            return owner;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "replacement owner did not appear"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn consume_default_first_run(binary: &str, home: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY)
        expect -exact "\x1b\[?1049h"
        after 400
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let status = expect_command()
        .args(["-c", script])
        .env("HOME", home)
        .env("PROQI_TEST_BINARY", binary)
        .env_remove("HERDR_ENV")
        .status()
        .expect("consume default first run");
    assert!(status.success());
    let sessions = json_default(binary, home, &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("first session ID");
    let _trashed = json_default(binary, home, &["sessions", "trash", session]);
    let _pruned = json_default(binary, home, &["sessions", "prune", session, "--yes"]);
}

fn json_default(binary: &str, home: &std::path::Path, arguments: &[&str]) -> serde_json::Value {
    let output = std::process::Command::new(binary)
        .arg("--json")
        .args(arguments)
        .env("HOME", home)
        .output()
        .expect("default-root JSON command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}
