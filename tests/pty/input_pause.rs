//! Input continuity and durability across child-only process suspension.

use std::{
    fmt::Write as _,
    fs,
    process::{Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, kill_process, test_kill_process};

use super::{
    support::{consume_first_run, expect_command, json_command},
    watchdog,
};

const CONTENT: &str = "wake-after-pause Grüße 界 + rapid\nsecond pause";
const FIRST_INPUT: &str = "wake-after-pause Grüße 界";
const WORKFLOW_LIMIT: Duration = Duration::from_secs(15);
const CREATE_WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 10
    set binary $env(PROQI_TEST_BINARY)
    set state $env(PROQI_TEST_STATE)
    set first [encoding convertfrom utf-8 [binary format H* $env(PROQI_TEST_FIRST_HEX)]]
    proc register_watchdog_pid {pid} {
        global env
        set owned [open $env(PROQI_TEST_PIDS) a]
        puts $owned $pid
        close $owned
    }
    spawn $binary --state-dir $state
    expect -exact "\x1b\[?1049h"
    register_watchdog_pid [exp_pid]
    set owned [open "$state/proqi-pid" w]
    puts $owned [exp_pid]
    close $owned
    while {![file exists "$state/first-resume"]} { after 10 }
    send -- "\x1b\[200~$first\x1b\[201~"
    send -- " + rapid"
    stty rows 7 columns 26
    stty rows 28 columns 100
    after 350
    set ready [open "$state/second-pause-ready" w]
    puts $ready ready
    close $ready
    while {![file exists "$state/second-resume"]} { after 10 }
    send -- "\x1b\[200~\nsecond pause\x1b\[201~"
    after 700
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[0 q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;
const RESUME_WORKFLOW: &str = r#"
    encoding system utf-8
    log_user 0
    set timeout 10
    proc register_watchdog_pid {pid} {
        global env
        set owned [open $env(PROQI_TEST_PIDS) a]
        puts $owned $pid
        close $owned
    }
    spawn $env(PROQI_TEST_BINARY) --state-dir $env(PROQI_TEST_STATE) -r $env(PROQI_TEST_SESSION)
    expect -exact "\x1b\[?1049h"
    register_watchdog_pid [exp_pid]
    send "\r"
    after 100
    send -- "\x1b\[F!"
    after 500
    send "\x1b"
    after 100
    send "q"
    expect -exact "\x1b\[0 q"
    expect -exact "\x1b\[?1049l"
    expect eof
    catch wait result
    exit [lindex $result 3]
"#;

#[test]
fn repeated_sigstop_grants_fresh_input_leases_and_preserves_exact_content() {
    let state = tempfile::tempdir().expect("temporary state");
    let binary = env!("CARGO_BIN_EXE_proqi");
    consume_first_run(binary, state.path());
    let first_hex = FIRST_INPUT
        .as_bytes()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(&mut hex, "{byte:02x}").expect("write input byte as hex");
            hex
        });
    let watchdog_pids = state.path().join("watchdog-pids");
    let mut create_command = expect_command();
    create_command
        .args(["-c", CREATE_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_FIRST_HEX", first_hex)
        .env("PROQI_TEST_PIDS", &watchdog_pids)
        .env_remove("HERDR_ENV");
    let mut workflow = WatchedWorkflow::spawn(
        create_command,
        watchdog_pids,
        state.path().join("proqi-pid"),
    );
    let process = workflow.read_process();
    pause_process(process, Duration::from_millis(750));
    fs::write(state.path().join("first-resume"), b"ready").expect("release first pause");
    wait_for_path(&workflow, &state.path().join("second-pause-ready"));
    pause_process(process, Duration::from_millis(650));
    fs::write(state.path().join("second-resume"), b"ready").expect("release second pause");
    let status = workflow.finish();
    assert!(status.success(), "suspended input PTY exited with {status}");

    let sessions = json_command(binary, state.path(), &["sessions", "list"]);
    let session = sessions["data"]["sessions"][0]["id"]
        .as_str()
        .expect("session ID");
    let thoughts = json_command(binary, state.path(), &["thoughts", "list", session]);
    let thought = thoughts["data"]["thoughts"][0]["id"]
        .as_str()
        .expect("thought ID");
    assert_eq!(thoughts["data"]["thoughts"][0]["content"], CONTENT);

    let resume_pids = state.path().join("resume-watchdog-pids");
    let mut resume_command = expect_command();
    resume_command
        .args(["-c", RESUME_WORKFLOW])
        .env("PROQI_TEST_BINARY", binary)
        .env("PROQI_TEST_STATE", state.path())
        .env("PROQI_TEST_SESSION", session)
        .env("PROQI_TEST_PIDS", &resume_pids)
        .env_remove("HERDR_ENV");
    let status = watchdog::status_before(
        &mut resume_command,
        WORKFLOW_LIMIT,
        &resume_pids,
        "resumed input PTY workflow",
    );
    assert!(status.success(), "resumed input PTY exited with {status}");

    let inspected = json_command(
        binary,
        state.path(),
        &["thoughts", "inspect", session, thought],
    );
    assert_eq!(
        inspected["data"]["thought"]["content"],
        format!("{CONTENT}!")
    );
}

struct WatchedWorkflow {
    watcher: Option<thread::JoinHandle<ExitStatus>>,
    process: Option<Pid>,
    process_path: std::path::PathBuf,
}

impl WatchedWorkflow {
    fn spawn(
        mut command: Command,
        watchdog_pids: std::path::PathBuf,
        process_path: std::path::PathBuf,
    ) -> Self {
        let watcher = thread::spawn(move || {
            watchdog::status_before(
                &mut command,
                WORKFLOW_LIMIT,
                &watchdog_pids,
                "suspended input PTY workflow",
            )
        });
        Self {
            watcher: Some(watcher),
            process: None,
            process_path,
        }
    }

    fn read_process(&mut self) -> Pid {
        let process_path = self.process_path.clone();
        wait_for_path(self, &process_path);
        let process = read_process(&process_path).expect("positive Proqi PID");
        self.process = Some(process);
        process
    }

    fn finish(mut self) -> ExitStatus {
        let status = self
            .watcher
            .take()
            .expect("active PTY watchdog")
            .join()
            .expect("PTY watchdog thread");
        self.process = None;
        status
    }

    fn watcher_finished(&self) -> bool {
        self.watcher
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
    }
}

impl Drop for WatchedWorkflow {
    fn drop(&mut self) {
        let process = self
            .process
            .take()
            .or_else(|| read_process(&self.process_path));
        if let Some(process) = process {
            let _continued = kill_process(process, Signal::CONT);
            let _terminated = kill_process(process, Signal::TERM);
        }
        // The watchdog thread owns an absolute deadline and kills every PID
        // registered by Expect before it returns, so this join is bounded.
        if let Some(watcher) = self.watcher.take() {
            let _settled = watcher.join();
        }
    }
}

fn pause_process(process: Pid, duration: Duration) {
    kill_process(process, Signal::STOP).expect("stop only the test Proqi child");
    thread::sleep(duration);
    assert!(
        test_kill_process(process).is_ok(),
        "test Proqi exited while stopped"
    );
    kill_process(process, Signal::CONT).expect("continue only the test Proqi child");
}

fn wait_for_path(workflow: &WatchedWorkflow, path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        assert!(!workflow.watcher_finished(), "PTY watchdog exited early");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        path.exists(),
        "PTY marker was not created before its deadline"
    );
}

fn read_process(path: &std::path::Path) -> Option<Pid> {
    fs::read_to_string(path)
        .ok()?
        .trim()
        .parse()
        .ok()
        .and_then(Pid::from_raw)
}
