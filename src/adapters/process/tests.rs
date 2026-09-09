use std::ffi::OsString;

#[cfg(unix)]
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{adapters::memory::FakeIdGenerator, ports::environment::IdGenerator as _};

#[cfg(unix)]
use crate::ports::environment::{ProcessError, ProcessRequest, ProcessRunner};

use super::resume_args;

#[cfg(unix)]
use super::{
    CancellationFlag, Deadline, OwnedChild, SystemProcessRunner, collect_execution,
    read_in_background, write_in_background,
};

#[test]
fn replacement_resume_arguments_preserve_an_explicit_state_root() {
    let mut ids = FakeIdGenerator::new(1_800_000_000_000);
    let session = ids.session_id();
    assert_eq!(
        resume_args(session, Some(std::path::Path::new("/private/state"))),
        ["--state-dir", "/private/state", "-r", &session.to_string(),].map(OsString::from)
    );
}

#[cfg(unix)]
#[test]
fn arguments_and_standard_input_are_not_shell_interpolated() {
    let mut runner = SystemProcessRunner::default();
    let output = runner
        .run(ProcessRequest {
            program: OsString::from("/bin/cat"),
            args: Vec::new(),
            stdin: Some(b"$(touch never) ; exact\n".to_vec()),
            timeout: Duration::from_secs(1),
        })
        .expect("direct process");
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(output.stdout, b"$(touch never) ; exact\n");
}

#[cfg(unix)]
#[test]
fn deadline_terminates_a_slow_process() {
    let mut runner = SystemProcessRunner::default();
    let result = runner.run(ProcessRequest {
        program: OsString::from("/bin/sleep"),
        args: vec![OsString::from("2")],
        stdin: None,
        timeout: Duration::from_millis(10),
    });
    assert_eq!(result, Err(ProcessError::TimedOut));
    let recovery = runner
        .run(ProcessRequest {
            program: OsString::from("/bin/cat"),
            args: Vec::new(),
            stdin: Some(b"recovered".to_vec()),
            timeout: Duration::from_secs(1),
        })
        .expect("later process remains usable");
    assert_eq!(recovery.stdout, b"recovered");
}

#[cfg(unix)]
#[test]
fn shared_cancellation_terminates_running_process_work() {
    let cancellation = CancellationFlag::default();
    let worker_cancellation = cancellation.clone();
    let handle = std::thread::spawn(move || {
        let mut runner = SystemProcessRunner::cancellable(worker_cancellation);
        runner.run(ProcessRequest {
            program: OsString::from("/bin/sleep"),
            args: vec![OsString::from("30")],
            stdin: None,
            timeout: Duration::from_secs(30),
        })
    });
    std::thread::sleep(Duration::from_millis(30));
    let started = Instant::now();
    cancellation.cancel();
    let result = handle.join().expect("process runner thread");
    assert_eq!(result, Err(ProcessError::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[cfg(unix)]
#[test]
fn inherited_output_pipe_cannot_extend_the_deadline() {
    let started = Instant::now();
    let mut runner = SystemProcessRunner::default();
    let result = runner.run(ProcessRequest {
        program: OsString::from("/bin/sh"),
        args: vec![OsString::from("-c"), OsString::from("sleep 2 &")],
        stdin: None,
        timeout: Duration::from_millis(30),
    });
    assert_eq!(result, Err(ProcessError::TimedOut));
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[cfg(unix)]
#[test]
fn timeout_terminates_a_ready_grandchild_that_inherits_output_pipes() {
    use std::os::unix::process::CommandExt as _;

    let temporary = tempfile::tempdir().expect("temporary fixture");
    let pid_file = temporary.path().join("grandchild.pid");
    let script = "sleep 30 & child=$!; echo $child > \"$1\"; wait";
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", script, "proqi-process-fixture"])
        .arg(&pid_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let child = command.spawn().expect("process-tree fixture");
    let mut child = OwnedChild::new(child);
    let grandchild = read_ready_pid(&pid_file);
    let stdin = child
        .child_mut()
        .stdin
        .take()
        .expect("child standard input");
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .expect("child standard output");
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .expect("child standard error");
    let settlement = collect_execution(
        &mut child,
        Deadline::new(Duration::from_millis(50)),
        &CancellationFlag::default(),
        write_in_background(stdin, None),
        read_in_background(stdout),
        read_in_background(stderr),
    );
    assert!(matches!(settlement.result, Err(ProcessError::TimedOut)));
    assert!(settlement.settled);
    assert_process_exits(grandchild);
}

#[cfg(unix)]
fn read_ready_pid(path: &std::path::Path) -> rustix::process::Pid {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| raw.trim().parse::<i32>().ok())
            .and_then(rustix::process::Pid::from_raw)
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "grandchild fixture did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn assert_process_exits(pid: rustix::process::Pid) {
    for _ in 0..50 {
        if rustix::process::test_kill_process(pid).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("grandchild survived process-tree cancellation");
}
