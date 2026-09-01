//! Panic-safe absolute wall-clock ownership for PTY driver processes.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex, TryLockError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rustix::{
    io::Errno,
    process::{Pid, Signal, WaitId, WaitIdOptions, kill_process, test_kill_process, waitid},
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const TERM_GRACE: Duration = Duration::from_millis(100);
const SETTLEMENT_RESERVE: Duration = Duration::from_millis(250);
const CLEANUP_RESERVE: Duration = Duration::from_millis(350);

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
    let deadline = Instant::now() + timeout;
    let child = command
        .spawn()
        .unwrap_or_else(|error| panic!("{context}: {error}"));
    let mut owned = OwnedChild::new(child, cleanup_pids.to_path_buf(), deadline);
    loop {
        if owned
            .driver_exited()
            .unwrap_or_else(|error| panic!("{context}: watchdog wait failed: {error}"))
        {
            let cleanup = owned.terminate();
            let output = owned.output();
            if !output.is_empty() {
                eprintln!("{output}");
            }
            assert!(
                cleanup.settled(),
                "{context}: driver exited but teardown did not settle before the absolute wall-clock limit of {timeout:?}: {cleanup:?}\n{output}"
            );
            return owned
                .status
                .expect("settled watchdog driver must have an exit status");
        }
        if Instant::now() >= owned.cleanup_at {
            let cleanup = owned.terminate();
            let output = owned.output();
            panic!(
                "{context}: exceeded absolute wall-clock limit of {timeout:?}; cleanup: {cleanup:?}\n{output}"
            );
        }
        thread::sleep(
            POLL_INTERVAL.min(owned.cleanup_at.saturating_duration_since(Instant::now())),
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct CleanupOutcome {
    driver: bool,
    descendants: bool,
    readers: bool,
}

impl CleanupOutcome {
    const fn settled(self) -> bool {
        self.driver && self.descendants && self.readers
    }
}

struct OwnedChild {
    child: Child,
    process: Pid,
    cleanup_pids: PathBuf,
    readers: Vec<OutputReader>,
    deadline: Instant,
    cleanup_at: Instant,
    force_at: Option<Instant>,
    term_sent: bool,
    kill_sent: bool,
    status: Option<ExitStatus>,
    armed: bool,
}

impl OwnedChild {
    fn new(mut child: Child, cleanup_pids: PathBuf, deadline: Instant) -> Self {
        let process = Pid::from_child(&child);
        let mut readers = Vec::with_capacity(2);
        if let Some(stdout) = child.stdout.take() {
            readers.push(OutputReader::spawn(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(OutputReader::spawn(stderr));
        }
        let now = Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        let reserve = CLEANUP_RESERVE.min(remaining / 2);
        let cleanup_at = deadline.checked_sub(reserve).unwrap_or(now);
        Self {
            child,
            process,
            cleanup_pids,
            readers,
            deadline,
            cleanup_at,
            force_at: None,
            term_sent: false,
            kill_sent: false,
            status: None,
            armed: true,
        }
    }

    fn terminate(&mut self) -> CleanupOutcome {
        if !self.armed {
            return self.outcome();
        }
        if !self.term_sent {
            self.signal_owned(Signal::TERM);
            self.term_sent = true;
            let reader_settlement_at = self
                .deadline
                .checked_sub(SETTLEMENT_RESERVE)
                .unwrap_or_else(Instant::now);
            self.force_at = Some((Instant::now() + TERM_GRACE).min(reader_settlement_at));
        }
        loop {
            let driver_exited = self.driver_exited().unwrap_or(false);
            let force_at = self.force_at.unwrap_or(self.deadline);
            if !self.kill_sent && (driver_exited || Instant::now() >= force_at) {
                // Keep Expect waitable until every final signal has been sent.
                // This prevents reuse of the driver PID and confines raw-PID
                // descendant signalling to the registered ownership window.
                self.signal_owned(Signal::KILL);
                self.kill_sent = true;
            }
            if self.kill_sent && driver_exited && self.status.is_none() {
                self.status = self.child.try_wait().ok().flatten();
            }
            self.poll_readers();
            let outcome = self.outcome();
            if outcome.settled() {
                self.armed = false;
                return outcome;
            }
            if Instant::now() >= self.deadline {
                return outcome;
            }
            thread::sleep(
                POLL_INTERVAL.min(self.deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn signal_owned(&self, signal: Signal) {
        for process in self.registered_processes() {
            let _signalled = kill_process(process, signal);
        }
        if self.status.is_none() {
            let _signalled = kill_process(self.process, signal);
        }
    }

    fn registered_processes(&self) -> Vec<Pid> {
        fs::read_to_string(&self.cleanup_pids)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse().ok())
            .filter_map(Pid::from_raw)
            .collect()
    }

    fn descendants_gone(&self) -> bool {
        self.registered_processes()
            .into_iter()
            .all(process_is_absent)
    }

    fn driver_exited(&self) -> rustix::io::Result<bool> {
        if self.status.is_some() {
            return Ok(true);
        }
        waitid(
            WaitId::Pid(self.process),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .map(|status| status.is_some())
    }

    fn poll_readers(&mut self) {
        for reader in &mut self.readers {
            reader.join_if_finished();
        }
    }

    fn outcome(&self) -> CleanupOutcome {
        CleanupOutcome {
            driver: self.status.is_some(),
            descendants: self.descendants_gone(),
            readers: self.readers.iter().all(OutputReader::settled),
        }
    }

    fn output(&self) -> String {
        let bytes = self
            .readers
            .iter()
            .flat_map(OutputReader::snapshot)
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _cleanup = self.terminate();
    }
}

struct OutputReader {
    bytes: Arc<Mutex<Vec<u8>>>,
    handle: Option<JoinHandle<()>>,
    failed: bool,
}

impl OutputReader {
    fn spawn(stream: impl std::io::Read + Send + 'static) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&bytes);
        let handle = thread::spawn(move || drain(stream, &output));
        Self {
            bytes,
            handle: Some(handle),
            failed: false,
        }
    }

    fn join_if_finished(&mut self) {
        if !self.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        let handle = self.handle.take().expect("finished watchdog reader");
        self.failed = handle.join().is_err();
    }

    fn settled(&self) -> bool {
        self.handle.is_none() && !self.failed
    }

    fn snapshot(&self) -> Vec<u8> {
        match self.bytes.try_lock() {
            Ok(bytes) => bytes.clone(),
            Err(TryLockError::Poisoned(error)) => error.into_inner().clone(),
            Err(TryLockError::WouldBlock) => b"<watchdog output reader still active>".to_vec(),
        }
    }
}

fn drain(mut stream: impl std::io::Read, output: &Mutex<Vec<u8>>) {
    let mut chunk = [0_u8; 1_024];
    while let Ok(read) = std::io::Read::read(&mut stream, &mut chunk) {
        if read == 0 {
            return;
        }
        let mut bytes = output
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let overflow = bytes
            .len()
            .saturating_add(read)
            .saturating_sub(OUTPUT_LIMIT);
        if overflow > 0 {
            let remove = overflow.min(bytes.len());
            bytes.drain(..remove);
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
}

fn process_is_absent(process: Pid) -> bool {
    matches!(test_kill_process(process), Err(Errno::SRCH | Errno::PERM))
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
                "/bin/sh -c 'trap \"\" TERM; while :; do printf output; count=0; while test $count -lt 10000; do count=$((count + 1)); done; done' & worker=$!; printf '%s\\n' \"$worker\" > \"$PROQI_TEST_PIDS\"; wait \"$worker\"",
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
        assert_registered_gone(&pids);
    }

    #[test]
    fn exited_driver_cannot_leave_an_output_inheriting_descendant_or_reader() {
        let state = tempfile::tempdir().expect("watchdog state");
        let pids = state.path().join("pids");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "/bin/sh -c 'trap \"\" HUP TERM; while :; do :; done' & worker=$!; printf '%s\\n' \"$worker\" > \"$PROQI_TEST_PIDS\"; exit 0",
            ])
            .env("PROQI_TEST_PIDS", &pids);
        let started = Instant::now();
        let status = status_before(
            &mut command,
            Duration::from_secs(1),
            &pids,
            "exited-driver inherited-output proof",
        );
        assert!(status.success());
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_registered_gone(&pids);
    }

    #[test]
    fn already_exited_registered_child_is_not_retained_as_owned_work() {
        let state = tempfile::tempdir().expect("watchdog state");
        let pids = state.path().join("pids");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "/bin/sh -c 'exit 0' & worker=$!; printf '%s\\n' \"$worker\" > \"$PROQI_TEST_PIDS\"; wait \"$worker\"; exit 0",
            ])
            .env("PROQI_TEST_PIDS", &pids);
        let status = status_before(
            &mut command,
            Duration::from_secs(1),
            &pids,
            "early descendant exit proof",
        );
        assert!(status.success());
        assert_registered_gone(&pids);
    }

    fn assert_registered_gone(pids: &std::path::Path) {
        let raw = std::fs::read_to_string(pids)
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
