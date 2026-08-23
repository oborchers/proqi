//! Direct, bounded child-process execution without a shell.

use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::ports::environment::{ProcessError, ProcessOutput, ProcessRequest, ProcessRunner};

const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;

/// Operating-system process runner with bounded output and a hard deadline.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessRunner;

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
        let stdout_reader = thread::spawn(move || read_bounded(stdout));
        let stderr_reader = thread::spawn(move || read_bounded(stderr));
        let stdin_writer = thread::spawn(move || write_input(stdin, request.stdin));
        let exit_code = wait_for_exit(&mut child, request.timeout)?;
        join_writer(stdin_writer)?;
        let stdout = join_reader(stdout_reader)?;
        let stderr = join_reader(stderr_reader)?;
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
    timeout: Duration,
) -> Result<Option<i32>, ProcessError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ProcessError::Io(error.to_string()))?
        {
            return Ok(status.code());
        }
        if Instant::now() >= deadline {
            let _killed = child.kill();
            let _waited = child.wait();
            return Err(ProcessError::TimedOut);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn join_reader(
    handle: thread::JoinHandle<std::io::Result<BoundedRead>>,
) -> Result<BoundedRead, ProcessError> {
    handle
        .join()
        .map_err(|_| ProcessError::Io("child output reader panicked".to_owned()))?
        .map_err(|error| ProcessError::Io(error.to_string()))
}

fn join_writer(handle: thread::JoinHandle<std::io::Result<()>>) -> Result<(), ProcessError> {
    handle
        .join()
        .map_err(|_| ProcessError::Io("child input writer panicked".to_owned()))?
        .map_err(|error| ProcessError::Io(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, time::Duration};

    use crate::ports::environment::{ProcessError, ProcessRequest, ProcessRunner};

    use super::SystemProcessRunner;

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
}
