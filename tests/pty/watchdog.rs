//! Panic-safe absolute wall-clock ownership for PTY driver processes.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, kill_process};

const OUTPUT_LIMIT: usize = 64 * 1024;

pub(super) fn status_before(
    command: &mut Command,
    timeout: Duration,
    cleanup_pids: &Path,
    context: &str,
) -> ExitStatus {
    // Expect must inherit the foreground process group on macOS. Explicit PID
    // registration supplies descendant ownership without changing PTY semantics.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command
        .spawn()
        .unwrap_or_else(|error| panic!("{context}: {error}"));
    let mut owned = OwnedChild::new(child, cleanup_pids.to_path_buf());
    let deadline = Instant::now() + timeout;
    loop {
        match owned
            .child
            .try_wait()
            .unwrap_or_else(|error| panic!("{context}: watchdog wait failed: {error}"))
        {
            Some(status) => {
                owned.armed = false;
                let output = owned.take_output();
                if !output.is_empty() {
                    eprintln!("{output}");
                }
                return status;
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                owned.terminate();
                let output = owned.take_output();
                panic!("{context}: exceeded absolute wall-clock limit of {timeout:?}\n{output}");
            }
        }
    }
}

struct OwnedChild {
    child: Child,
    process: Pid,
    cleanup_pids: PathBuf,
    output: Vec<JoinHandle<Vec<u8>>>,
    armed: bool,
}

impl OwnedChild {
    fn new(mut child: Child, cleanup_pids: PathBuf) -> Self {
        let process = Pid::from_child(&child);
        let mut output = Vec::with_capacity(2);
        if let Some(stdout) = child.stdout.take() {
            output.push(drain(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            output.push(drain(stderr));
        }
        Self {
            child,
            process,
            cleanup_pids,
            output,
            armed: true,
        }
    }

    fn terminate(&mut self) {
        if !self.armed {
            return;
        }
        self.signal_registered(Signal::TERM);
        let _terminated = kill_process(self.process, Signal::TERM);
        let driver_reaped = self.reap_before(Duration::from_millis(250));
        self.signal_registered(Signal::KILL);
        if !driver_reaped {
            let _killed = kill_process(self.process, Signal::KILL);
            let _reaped = self.reap_before(Duration::from_millis(250));
        }
        self.armed = false;
    }

    fn signal_registered(&self, signal: Signal) {
        let Ok(contents) = fs::read_to_string(&self.cleanup_pids) else {
            return;
        };
        for process in contents
            .lines()
            .filter_map(|line| line.parse().ok())
            .filter_map(Pid::from_raw)
        {
            let _signalled = kill_process(process, signal);
        }
    }

    fn take_output(&mut self) -> String {
        let bytes = self
            .output
            .drain(..)
            .filter_map(|reader| reader.join().ok())
            .flatten()
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn reap_before(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(5));
                }
                Ok(None) | Err(_) => return false,
            }
        }
    }
}

fn drain(mut stream: impl std::io::Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1_024];
        while let Ok(read) = std::io::Read::read(&mut stream, &mut chunk) {
            if read == 0 {
                break;
            }
            if read >= OUTPUT_LIMIT {
                bytes.clear();
                bytes.extend_from_slice(&chunk[read - OUTPUT_LIMIT..read]);
                continue;
            }
            let overflow = bytes
                .len()
                .saturating_add(read)
                .saturating_sub(OUTPUT_LIMIT);
            if overflow > 0 {
                bytes.drain(..overflow);
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        bytes
    })
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        process::Command,
        time::{Duration, Instant},
    };

    use rustix::process::{Pid, test_kill_process};

    use super::status_before;

    #[test]
    fn continuous_output_cannot_defeat_the_absolute_watchdog_or_child_cleanup() {
        let state = tempfile::tempdir().expect("watchdog state");
        let pids = state.path().join("pids");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "/bin/sh -c 'while :; do printf output; count=0; while test $count -lt 10000; do count=$((count + 1)); done; done' & worker=$!; printf '%s\\n' \"$worker\" > \"$PROQI_TEST_PIDS\"; wait \"$worker\"",
            ])
            .env("PROQI_TEST_PIDS", &pids);
        let started = Instant::now();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _status = status_before(
                &mut command,
                Duration::from_millis(100),
                &pids,
                "continuous-output watchdog proof",
            );
        }));
        assert!(panic.is_err(), "watchdog command unexpectedly completed");
        assert!(started.elapsed() < Duration::from_secs(1));

        let raw = std::fs::read_to_string(&pids)
            .expect("registered child")
            .trim()
            .parse()
            .expect("child pid");
        let child = Pid::from_raw(raw).expect("positive child pid");
        let deadline = Instant::now() + Duration::from_secs(1);
        while test_kill_process(child).is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            test_kill_process(child).is_err(),
            "watchdog left its registered child alive"
        );
    }
}
