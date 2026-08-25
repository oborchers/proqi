//! Direct, bounded child-process execution without a shell.

use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use crate::ports::environment::{ProcessError, ProcessOutput, ProcessRequest, ProcessRunner};

const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;

/// Operating-system process runner with bounded output and a hard deadline.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessRunner;

/// Unix process-image replacement after explicit caller-owned cleanup.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessReplacer;

#[cfg(unix)]
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

#[cfg(not(unix))]
impl crate::ports::update::ProcessReplacer for SystemProcessReplacer {
    fn replace(
        &self,
        _executable: &std::path::Path,
        _session_id: crate::domain::SessionId,
        _state_root: Option<&std::path::Path>,
    ) -> Result<(), crate::ports::update::UpdateError> {
        Err(crate::ports::update::UpdateError::Coordination(
            "process replacement is unsupported on this platform".to_owned(),
        ))
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
        let mut command = Command::new(request.program);
        command
            .args(request.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| ProcessError::Io(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ProcessError::Io("child standard input is unavailable".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProcessError::Io("child standard output is unavailable".to_owned()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProcessError::Io("child standard error is unavailable".to_owned()))?;
        let stdout_reader = read_in_background(stdout);
        let stderr_reader = read_in_background(stderr);
        let stdin_writer = write_in_background(stdin, request.stdin);
        let deadline = Deadline::new(request.timeout);
        let exit_code = wait_for_exit(&mut child, deadline)?;
        receive_writer(&stdin_writer, deadline)?;
        let stdout = receive_reader(&stdout_reader, deadline)?;
        let stderr = receive_reader(&stderr_reader, deadline)?;
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

fn read_in_background(
    reader: impl Read + Send + 'static,
) -> Receiver<std::io::Result<BoundedRead>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _sent = sender.send(read_bounded(reader));
    });
    receiver
}

fn write_in_background(
    writer: impl Write + Send + 'static,
    input: Option<Vec<u8>>,
) -> Receiver<std::io::Result<()>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _sent = sender.send(write_input(writer, input));
    });
    receiver
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
    child: &mut std::process::Child,
    deadline: Deadline,
) -> Result<Option<i32>, ProcessError> {
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ProcessError::Io(error.to_string()))?
        {
            return Ok(status.code());
        }
        if deadline.remaining().is_none() {
            let _killed = child.kill();
            let _waited = child.wait();
            return Err(ProcessError::TimedOut);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn receive_reader(
    receiver: &Receiver<std::io::Result<BoundedRead>>,
    deadline: Deadline,
) -> Result<BoundedRead, ProcessError> {
    receive_until(receiver, deadline, "child output reader")
}

fn receive_writer(
    receiver: &Receiver<std::io::Result<()>>,
    deadline: Deadline,
) -> Result<(), ProcessError> {
    receive_until(receiver, deadline, "child input writer")
}

fn receive_until<T>(
    receiver: &Receiver<std::io::Result<T>>,
    deadline: Deadline,
    worker: &str,
) -> Result<T, ProcessError> {
    let Some(remaining) = deadline.remaining() else {
        return Err(ProcessError::TimedOut);
    };
    match receiver.recv_timeout(remaining) {
        Ok(result) => result.map_err(|error| ProcessError::Io(error.to_string())),
        Err(RecvTimeoutError::Timeout) => Err(ProcessError::TimedOut),
        Err(RecvTimeoutError::Disconnected) => {
            Err(ProcessError::Io(format!("{worker} stopped unexpectedly")))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    #[cfg(unix)]
    use std::time::Duration;

    use crate::{adapters::memory::FakeIdGenerator, ports::environment::IdGenerator as _};

    #[cfg(unix)]
    use crate::ports::environment::{ProcessError, ProcessRequest, ProcessRunner};

    use super::resume_args;

    #[cfg(unix)]
    use super::SystemProcessRunner;

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
        let mut runner = SystemProcessRunner;
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
        let mut runner = SystemProcessRunner;
        let result = runner.run(ProcessRequest {
            program: OsString::from("/bin/sleep"),
            args: vec![OsString::from("2")],
            stdin: None,
            timeout: Duration::from_millis(10),
        });
        assert_eq!(result, Err(ProcessError::TimedOut));
    }

    #[cfg(unix)]
    #[test]
    fn inherited_output_pipe_cannot_extend_the_deadline() {
        let started = std::time::Instant::now();
        let mut runner = SystemProcessRunner;
        let result = runner.run(ProcessRequest {
            program: OsString::from("/bin/sh"),
            args: vec![OsString::from("-c"), OsString::from("sleep 2 &")],
            stdin: None,
            timeout: Duration::from_millis(30),
        });
        assert_eq!(result, Err(ProcessError::TimedOut));
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
