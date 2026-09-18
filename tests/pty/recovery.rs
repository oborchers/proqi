//! Recovery-mode export, failure truth, and explicit Board-mode shutdown.

use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    panic::{AssertUnwindSafe, catch_unwind},
    process::{Command, ExitStatus},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use proqi::ports::recovery::RecoveryDocument;
use rustix::process::{Pid, test_kill_process};

use super::{
    support::{consume_first_run, expect_command, json_command},
    watchdog,
};

const RECOVERY_WORKFLOW_LIMIT: Duration = Duration::from_secs(45);
const TIMEOUT_PROOF_LIMIT: Duration = Duration::from_secs(15);

const RECOVERY_WORKFLOW: &str = r#"
    log_user 0
    set timeout 10
    set binary $env(PROQI_TEST_BINARY)
    set state $env(PROQI_TEST_STATE)
    set session $env(PROQI_TEST_SESSION)
    spawn $binary --state-dir $state -r $session
    expect -exact "\x1b\[?1049h"
    set owned [open $env(PROQI_TEST_PIDS) w]
    puts $owned [exp_pid]
    close $owned
    set ready_marker [open "$state/terminal-ready" w]
    close $ready_marker
    while {![file exists "$state/immutable-ready"]} {
        expect -timeout 0 {
            -re ".+" { exp_continue }
            timeout {}
            eof { exit 123 }
        }
        after 25
    }
    if {[info exists env(PROQI_TEST_RECOVERY_HANG_AFTER_READY)]} {
        while {1} { after 1000 }
    }
    send -- "\x1b\[200~recovery-quit-sentinel\x1b\[201~"
    expect -re "storage I/O failed"
    send "w"
    expect -re "exporting recovery file"
    expect -re "recovery exported"
    send "q"
    expect {
        eof {}
        timeout {
            exit 124
        }
    }
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn exported_save_failure_accepts_the_raw_board_quit_key() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let session = prepare_recovery_session(binary, state.path());
    let database = state.path().join("data/proqi.sqlite3");
    let terminal_ready = state.path().join("terminal-ready");
    let ready = state.path().join("immutable-ready");
    let pids = state.path().join("recovery-watchdog-pids");
    let workflow = RecoveryWorkflow::spawn(
        recovery_command(binary, state.path(), &session, &pids, false),
        pids,
        RECOVERY_WORKFLOW_LIMIT,
    );
    workflow.wait_for_ready(&terminal_ready);
    assert!(
        database.exists(),
        "database exists after terminal readiness"
    );
    let immutable = ImmutableGuard::set(database);
    fs::write(ready, b"ready").expect("signal immutable database");
    let status = workflow.finish();
    drop(immutable);
    assert!(status.success(), "recovery PTY failed: {status}");

    let recovery = state.path().join("data/recovery");
    let paths = fs::read_dir(recovery)
        .expect("recovery directory")
        .map(|entry| entry.expect("recovery entry").path())
        .collect::<Vec<_>>();
    let [path] = paths.as_slice() else {
        panic!("expected one recovery export, found {}", paths.len());
    };
    let document: RecoveryDocument =
        serde_json::from_slice(&fs::read(path).expect("read recovery export"))
            .expect("decode recovery export");
    assert!(
        document
            .thoughts
            .iter()
            .any(|thought| thought.content == "recovery-quit-sentinel")
    );
    assert_eq!(
        fs::metadata(path)
            .expect("recovery metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn recovery_watchdog_cleans_registered_process_after_post_ready_hang() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    let session = prepare_recovery_session(binary, state.path());
    let database = state.path().join("data/proqi.sqlite3");
    let terminal_ready = state.path().join("terminal-ready");
    let ready = state.path().join("immutable-ready");
    let pids = state.path().join("recovery-watchdog-pids");
    let workflow = RecoveryWorkflow::spawn(
        recovery_command(binary, state.path(), &session, &pids, true),
        pids.clone(),
        TIMEOUT_PROOF_LIMIT,
    );
    workflow.wait_for_ready(&terminal_ready);
    let immutable = ImmutableGuard::set(database);
    fs::write(ready, b"ready").expect("release recovery hang injection");
    let timeout = catch_unwind(AssertUnwindSafe(|| workflow.finish()));
    drop(immutable);
    assert!(timeout.is_err(), "recovery watchdog unexpectedly completed");
    assert_registered_gone(&pids);
}

struct RecoveryWorkflow {
    watcher: Option<JoinHandle<ExitStatus>>,
}

impl RecoveryWorkflow {
    fn spawn(mut command: Command, pids: std::path::PathBuf, limit: Duration) -> Self {
        let watcher = thread::spawn(move || {
            watchdog::status_before(&mut command, limit, &pids, "recovery PTY workflow")
        });
        Self {
            watcher: Some(watcher),
        }
    }

    fn wait_for_ready(&self, path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            assert!(
                !self.watcher_finished(),
                "recovery PTY watchdog exited before terminal readiness"
            );
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            path.exists(),
            "recovery PTY did not become ready before its deadline"
        );
    }

    fn finish(mut self) -> ExitStatus {
        self.watcher
            .take()
            .expect("active recovery PTY watchdog")
            .join()
            .expect("recovery PTY watchdog thread")
    }

    fn watcher_finished(&self) -> bool {
        self.watcher.as_ref().is_some_and(JoinHandle::is_finished)
    }
}

impl Drop for RecoveryWorkflow {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            let _settled = watcher.join();
        }
    }
}

struct ImmutableGuard(std::path::PathBuf);

impl ImmutableGuard {
    fn set(path: std::path::PathBuf) -> Self {
        let status = Command::new("chflags")
            .args(["uchg", path.to_str().expect("UTF-8 database path")])
            .status()
            .expect("set immutable database");
        assert!(status.success());
        Self(path)
    }
}

impl Drop for ImmutableGuard {
    fn drop(&mut self) {
        let _status = Command::new("chflags").arg("nouchg").arg(&self.0).status();
    }
}

fn prepare_recovery_session(binary: &str, state: &std::path::Path) -> String {
    consume_first_run(binary, state);
    create_empty_session(binary, state);
    json_command(binary, state, &["sessions", "list"])["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID")
        .to_owned()
}

fn recovery_command(
    binary: &str,
    state: &std::path::Path,
    session: &str,
    pids: &std::path::Path,
    inject_hang: bool,
) -> Command {
    let mut command = expect_command();
    command
        .args(["-c", RECOVERY_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_PIDS", pids)
        .env_remove("HERDR_ENV");
    if inject_hang {
        command.env("PROQI_TEST_RECOVERY_HANG_AFTER_READY", "1");
    }
    command
}

fn assert_registered_gone(pids: &std::path::Path) {
    let raw = fs::read_to_string(pids)
        .expect("registered recovery child")
        .trim()
        .parse()
        .expect("recovery child PID");
    let child = Pid::from_raw(raw).expect("positive recovery child PID");
    let deadline = Instant::now() + Duration::from_secs(1);
    while test_kill_process(child).is_ok() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        test_kill_process(child).is_err(),
        "recovery watchdog left its registered child alive"
    );
}

fn create_empty_session(binary: &str, state: &std::path::Path) {
    let script = r#"
        log_user 0
        set timeout 10
        spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE)
        expect -exact "\x1b\[?1049h"
        set owned [open $env(PROQI_TEST_PIDS) w]
        puts $owned [exp_pid]
        close $owned
        send "\x1b"
        after 50
        send "q"
        expect eof
        catch wait result
        exit [lindex $result 3]
    "#;
    let pids = state.join("empty-session-watchdog-pids");
    let mut command = expect_command();
    command
        .args(["-c", script])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state)
        .env("PROQI_TEST_PIDS", &pids)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut command,
        RECOVERY_WORKFLOW_LIMIT,
        &pids,
        "empty recovery session PTY workflow",
    );
    assert!(status.success());
}
