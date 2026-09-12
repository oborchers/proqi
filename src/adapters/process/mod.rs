//! Direct, bounded child-process execution without a shell.

mod cancellation;
mod owned_child;

use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::ports::environment::{ProcessError, ProcessOutput, ProcessRequest, ProcessRunner};

pub(crate) use cancellation::CancellationFlag;
use owned_child::OwnedChild;

/// Maximum bytes captured independently from child-process stdout or stderr.
pub const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;
const TERMINATION_GRACE: Duration = Duration::from_millis(250);

/// Operating-system process runner with bounded output and a hard deadline.
#[derive(Debug, Default)]
pub struct SystemProcessRunner {
    cancellation: CancellationFlag,
    cleanup_incomplete: bool,
}

impl Clone for SystemProcessRunner {
    fn clone(&self) -> Self {
        Self {
            cancellation: self.cancellation.clone(),
            cleanup_incomplete: self.cleanup_incomplete,
        }
    }
}

impl SystemProcessRunner {
    pub(crate) fn cancellable(cancellation: CancellationFlag) -> Self {
        Self {
            cancellation,
            cleanup_incomplete: false,
        }
    }

    pub(crate) fn cancellation(&self) -> CancellationFlag {
        self.cancellation.clone()
    }
}

/// Unix process-image replacement after explicit caller-owned cleanup.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessReplacer;

impl crate::ports::update::ProcessReplacer for SystemProcessReplacer {
    fn replace(
        &self,
        executable: &std::path::Path,
        session_id: crate::domain::SessionId,
        state_root: Option<&std::path::Path>,
    ) -> Result<(), crate::ports::update::UpdateError> {
        use std::os::unix::process::CommandExt as _;

        let mut command = Command::new(executable);
        command.args(resume_args(session_id, state_root));
        let error = command.exec();
        Err(crate::ports::update::UpdateError::Coordination(format!(
            "process replacement failed: {error}"
        )))
    }
}

fn resume_args(
    session_id: crate::domain::SessionId,
    state_root: Option<&std::path::Path>,
) -> Vec<std::ffi::OsString> {
    let mut arguments = Vec::new();
    if let Some(root) = state_root {
        arguments.push("--state-dir".into());
        arguments.push(root.as_os_str().to_owned());
    }
    arguments.push("-r".into());
    arguments.push(session_id.to_string().into());
    arguments
}

impl ProcessRunner for SystemProcessRunner {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        if self.cleanup_incomplete {
            return Err(ProcessError::Io(
                "previous child-process cleanup did not settle".to_owned(),
            ));
        }
        if self.cancellation.is_cancelled() {
            return Err(ProcessError::Cancelled);
        }
        let mut command = Command::new(request.program);
        command
            .args(request.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        let child = command
            .spawn()
            .map_err(|error| ProcessError::Io(error.to_string()))?;
        let mut child = OwnedChild::new(child);
        let stdin =
            child.child_mut().stdin.take().ok_or_else(|| {
                ProcessError::Io("child standard input is unavailable".to_owned())
            })?;
        let stdout =
            child.child_mut().stdout.take().ok_or_else(|| {
                ProcessError::Io("child standard output is unavailable".to_owned())
            })?;
        let stderr =
            child.child_mut().stderr.take().ok_or_else(|| {
                ProcessError::Io("child standard error is unavailable".to_owned())
            })?;
        let stdout_reader = read_in_background(stdout);
        let stderr_reader = read_in_background(stderr);
        let stdin_writer = write_in_background(stdin, request.stdin);
        let deadline = Deadline::new(request.timeout);
        let settlement = collect_execution(
            &mut child,
            deadline,
            &self.cancellation,
            stdin_writer,
            stdout_reader,
            stderr_reader,
        );
        self.cleanup_incomplete = !settlement.settled;
        let execution = settlement.result?;
        child.disarm();
        let exit_code = execution.exit_code;
        let stdout = execution.stdout;
        let stderr = execution.stderr;
        if stdout.exceeded || stderr.exceeded {
            return Err(ProcessError::OutputLimit);
        }
        Ok(ProcessOutput {
            exit_code,
            stdout: stdout.bytes,
            stderr: stderr.bytes,
        })
    }
}

#[derive(Clone, Copy)]
struct Deadline {
    started: Instant,
    timeout: Duration,
}

impl Deadline {
    fn new(timeout: Duration) -> Self {
        Self {
            started: Instant::now(),
            timeout,
        }
    }

    fn remaining(self) -> Option<Duration> {
        self.timeout.checked_sub(self.started.elapsed())
    }
}

struct Worker<T> {
    receiver: Receiver<std::io::Result<T>>,
    handle: Option<JoinHandle<()>>,
}

fn read_in_background(reader: impl Read + Send + 'static) -> Worker<BoundedRead> {
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let _sent = sender.send(read_bounded(reader));
    });
    Worker {
        receiver,
        handle: Some(handle),
    }
}

fn write_in_background(writer: impl Write + Send + 'static, input: Option<Vec<u8>>) -> Worker<()> {
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let _sent = sender.send(write_input(writer, input));
    });
    Worker {
        receiver,
        handle: Some(handle),
    }
}

fn write_input(mut writer: impl Write, input: Option<Vec<u8>>) -> std::io::Result<()> {
    if let Some(input) = input {
        writer.write_all(&input)?;
    }
    Ok(())
}

struct BoundedRead {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn read_bounded(reader: impl Read) -> std::io::Result<BoundedRead> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_CAPTURE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    let exceeded = u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CAPTURE_BYTES;
    if exceeded {
        bytes.truncate(usize::try_from(MAX_CAPTURE_BYTES).unwrap_or(usize::MAX));
    }
    Ok(BoundedRead { bytes, exceeded })
}

fn wait_for_exit(
    child: &mut Child,
    deadline: Deadline,
    cancellation: &CancellationFlag,
) -> Result<Option<i32>, ProcessError> {
    loop {
        if cancellation.is_cancelled() {
            return Err(ProcessError::Cancelled);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ProcessError::Io(error.to_string()))?
        {
            return Ok(status.code());
        }
        if deadline.remaining().is_none() {
            return Err(ProcessError::TimedOut);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

struct ExecutionOutput {
    exit_code: Option<i32>,
    stdout: BoundedRead,
    stderr: BoundedRead,
}

struct ExecutionSettlement {
    result: Result<ExecutionOutput, ProcessError>,
    settled: bool,
}

fn collect_execution(
    child: &mut OwnedChild,
    deadline: Deadline,
    cancellation: &CancellationFlag,
    mut stdin: Worker<()>,
    mut stdout: Worker<BoundedRead>,
    mut stderr: Worker<BoundedRead>,
) -> ExecutionSettlement {
    let result = wait_for_exit(child.child_mut(), deadline, cancellation).and_then(|exit_code| {
        receive_worker(&mut stdin, deadline, "child input writer")?;
        let output = receive_worker(&mut stdout, deadline, "child output reader")?;
        let errors = receive_worker(&mut stderr, deadline, "child error reader")?;
        Ok((exit_code, output, errors))
    });
    let child_settled = result.is_ok() || child.terminate();
    let cleanup = Deadline::new(TERMINATION_GRACE);
    let joins = [
        finish_worker(&mut stdin, cleanup),
        finish_worker(&mut stdout, cleanup),
        finish_worker(&mut stderr, cleanup),
    ];
    let workers_settled = joins.iter().all(Result::is_ok);
    let result = result.and_then(|(exit_code, stdout, stderr)| {
        for joined in joins {
            joined?;
        }
        Ok(ExecutionOutput {
            exit_code,
            stdout,
            stderr,
        })
    });
    ExecutionSettlement {
        result,
        settled: child_settled && workers_settled,
    }
}

fn receive_worker<T>(
    worker: &mut Worker<T>,
    deadline: Deadline,
    label: &str,
) -> Result<T, ProcessError> {
    let Some(remaining) = deadline.remaining() else {
        return Err(ProcessError::TimedOut);
    };
    match worker.receiver.recv_timeout(remaining) {
        Ok(result) => result.map_err(|error| ProcessError::Io(error.to_string())),
        Err(RecvTimeoutError::Timeout) => Err(ProcessError::TimedOut),
        Err(RecvTimeoutError::Disconnected) => {
            Err(ProcessError::Io(format!("{label} stopped unexpectedly")))
        }
    }
}

fn finish_worker<T>(worker: &mut Worker<T>, deadline: Deadline) -> Result<(), ProcessError> {
    while worker
        .handle
        .as_ref()
        .is_some_and(|handle| !handle.is_finished())
        && deadline.remaining().is_some()
    {
        thread::sleep(Duration::from_millis(2));
    }
    let Some(handle) = worker.handle.take() else {
        return Ok(());
    };
    if !handle.is_finished() {
        return Err(ProcessError::TimedOut);
    }
    handle
        .join()
        .map_err(|_| ProcessError::Io("child I/O worker panicked".to_owned()))
}

#[cfg(test)]
mod tests;
