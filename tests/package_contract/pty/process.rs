//! Panic-safe ownership of an installed product running under a Unix PTY.

use std::{
    fs,
    io::{Read, Write},
    sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, ExitStatus, PtySize, native_pty_system};
use rustix::process::{
    Pid, Signal, WaitId, WaitIdOptions, kill_process, kill_process_group, waitid,
};
use serde_json::Value;

use crate::InstalledProduct;

#[path = "process/cleanup.rs"]
mod cleanup;

use cleanup::{
    CleanupOutcome, CleanupProgress, CleanupState, TeardownDeadline, process_group_is_absent,
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(3);
// Match the production terminal shutdown budget before forcing the owned group.
const TERM_GRACE: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(super) struct PtyChild {
    child: Option<Box<dyn Child + Send + Sync>>,
    process_group: Option<Pid>,
    input: Option<Box<dyn Write + Send>>,
    output: Arc<Mutex<Vec<u8>>>,
    reader: Option<thread::JoinHandle<()>>,
    exit_status: Option<ExitStatus>,
    cleanup: Option<CleanupProgress>,
    #[cfg(test)]
    drop_success: Option<Arc<AtomicBool>>,
}

impl PtyChild {
    pub(super) fn spawn(product: &InstalledProduct, session: &str) -> Self {
        let mut command = CommandBuilder::new(&product.binary);
        command.env_clear();
        command.arg("--state-dir");
        command.arg(&product.state);
        command.args(["-r", session]);
        command.cwd(&product.working);
        command.env("PROQI_DISABLE_HERDR", "1");
        command.env("NO_PROXY", "*");
        command.env("HTTP_PROXY", "http://127.0.0.1:1");
        command.env("HTTPS_PROXY", "http://127.0.0.1:1");
        command.env("TERM", "xterm-256color");
        Self::spawn_command(command)
    }

    fn spawn_command(command: CommandBuilder) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open package PTY");
        let reader = pair.master.try_clone_reader().expect("clone PTY reader");
        let input = pair.master.take_writer().expect("take PTY writer");
        let child = pair
            .slave
            .spawn_command(command)
            .expect("spawn installed PTY owner");
        let process_group = child_pid(child.as_ref());
        let output = Arc::new(Mutex::new(Vec::new()));
        let reader_output = Arc::clone(&output);
        // Arm ownership before inspecting any fallible PID or PTY metadata.
        let mut owned = Self {
            child: Some(child),
            process_group,
            input: Some(input),
            output,
            reader: None,
            exit_status: None,
            cleanup: None,
            #[cfg(test)]
            drop_success: None,
        };
        let process = owned
            .child
            .as_ref()
            .and_then(|child| child_pid(child.as_ref()))
            .expect("PTY process ID");
        let process_group = pair
            .master
            .process_group_leader()
            .and_then(Pid::from_raw)
            .expect("dedicated PTY process group");
        assert_eq!(
            process_group, process,
            "portable-pty did not isolate the package owner as its group leader"
        );
        drop(pair.slave);
        match thread::Builder::new()
            .name("package-pty-output".to_owned())
            .spawn(move || read_pty(reader, &reader_output))
        {
            Ok(reader) => owned.reader = Some(reader),
            Err(error) => {
                let cleanup = owned.terminate();
                panic!("spawn package PTY reader: {error}; cleanup: {cleanup:?}");
            }
        }
        owned
    }

    #[cfg(test)]
    fn observe_drop_success(&mut self) -> Arc<AtomicBool> {
        let success = Arc::new(AtomicBool::new(false));
        self.drop_success = Some(Arc::clone(&success));
        success
    }

    pub(super) fn input(&mut self) -> &mut dyn Write {
        self.input.as_deref_mut().expect("live PTY input")
    }

    pub(super) fn process_id(&self) -> u32 {
        self.child
            .as_ref()
            .and_then(|child| child.process_id())
            .expect("PTY process ID")
    }

    pub(super) fn wait_for_owner(&mut self, product: &InstalledProduct, session: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if owner_is_ready(product, session) {
                return;
            }
            if self.child_has_exited().expect("poll starting owner") {
                let cleanup = self.terminate();
                let status = self.exit_status.as_ref().expect("reaped PTY owner");
                panic!(
                    "installed owner exited with {status}; cleanup: {cleanup:?}: {}",
                    String::from_utf8_lossy(&self.output())
                );
            }
            assert!(
                Instant::now() < deadline,
                "installed owner did not start: {}",
                String::from_utf8_lossy(&self.output())
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub(super) fn finish(self, timeout: Duration) -> Vec<u8> {
        let (success, output) = self.finish_with_status(timeout);
        assert!(
            success,
            "PTY owner exited unsuccessfully: {}",
            String::from_utf8_lossy(&output)
        );
        output
    }

    pub(super) fn finish_with_status(mut self, timeout: Duration) -> (bool, Vec<u8>) {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child_has_exited().expect("poll PTY child") {
                true => {
                    let cleanup = self.terminate();
                    assert!(
                        cleanup.successful(),
                        "PTY teardown exceeded its deadline: {cleanup:?}: {}",
                        String::from_utf8_lossy(&self.output())
                    );
                    let success = self
                        .exit_status
                        .as_ref()
                        .expect("reaped PTY owner")
                        .success();
                    return (success, self.output());
                }
                false if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                false => {
                    let cleanup = self.terminate();
                    panic!(
                        "PTY owner did not exit within {timeout:?}; cleanup: {cleanup:?}: {}",
                        String::from_utf8_lossy(&self.output())
                    );
                }
            }
        }
    }

    fn child_has_exited(&self) -> rustix::io::Result<bool> {
        let process = self
            .child
            .as_ref()
            .and_then(|child| child_pid(child.as_ref()));
        let Some(process) = process else {
            return Ok(true);
        };
        waitid(
            WaitId::Pid(process),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .map(|status| status.is_some())
    }

    fn output(&self) -> Vec<u8> {
        match self.output.try_lock() {
            Ok(output) => output.clone(),
            Err(TryLockError::Poisoned(error)) => error.into_inner().clone(),
            Err(TryLockError::WouldBlock) => b"<PTY output reader still active>".to_vec(),
        }
    }

    fn terminate(&mut self) -> CleanupOutcome {
        self.input.take();
        if self.cleanup.is_none() {
            let deadline = TeardownDeadline::after(TEARDOWN_TIMEOUT);
            self.cleanup = Some(CleanupProgress {
                deadline,
                force_at: deadline.capped_after(TERM_GRACE),
                term_sent: false,
                kill_sent: false,
                outcome: CleanupOutcome::new(self),
            });
        }
        if !self
            .cleanup
            .as_ref()
            .is_some_and(|cleanup| cleanup.term_sent)
        {
            self.signal_owned(Signal::TERM);
            self.cleanup.as_mut().expect("cleanup progress").term_sent = true;
        }
        loop {
            self.poll_reader();
            let child_exited = self.child_has_exited().unwrap_or(false);
            let cleanup = self.cleanup.as_ref().expect("cleanup progress");
            if !cleanup.kill_sent && (cleanup.force_at.expired() || child_exited) {
                // Keep the direct leader waitable until the final group signal
                // has been sent, so its PID cannot be reused as a foreign PGID.
                self.signal_owned(Signal::KILL);
                self.cleanup.as_mut().expect("cleanup progress").kill_sent = true;
            }
            if self
                .cleanup
                .as_ref()
                .is_some_and(|cleanup| cleanup.kill_sent)
            {
                let child = self.poll_child();
                self.cleanup
                    .as_mut()
                    .expect("cleanup progress")
                    .outcome
                    .child = child;
            }
            self.poll_group();
            let cleanup = self.cleanup.as_ref().expect("cleanup progress");
            if cleanup.outcome.settled() || cleanup.deadline.expired() {
                return cleanup.outcome;
            }
            thread::sleep(POLL_INTERVAL.min(cleanup.next_deadline().remaining()));
        }
    }

    fn signal_owned(&self, signal: Signal) {
        let group_signalled = self
            .process_group
            .is_some_and(|group| kill_process_group(group, signal).is_ok());
        if !group_signalled
            && let Some(process) = self
                .child
                .as_ref()
                .and_then(|child| child_pid(child.as_ref()))
        {
            let _ = kill_process(process, signal);
        }
    }

    fn poll_reader(&mut self) {
        let reader = self.join_finished_reader();
        if let Some(reader) = reader {
            self.cleanup
                .as_mut()
                .expect("cleanup progress")
                .outcome
                .reader = reader;
        }
    }

    fn poll_group(&mut self) {
        let group = if self.process_group.is_none_or(process_group_is_absent) {
            self.process_group.take();
            CleanupState::Complete
        } else {
            CleanupState::Pending
        };
        self.cleanup
            .as_mut()
            .expect("cleanup progress")
            .outcome
            .group = group;
    }

    fn poll_child(&mut self) -> CleanupState {
        let Some(child) = self.child.as_mut() else {
            return CleanupState::Complete;
        };
        if let Ok(Some(status)) = child.try_wait() {
            self.exit_status = Some(status);
            self.child.take();
            CleanupState::Complete
        } else {
            CleanupState::Pending
        }
    }

    fn join_finished_reader(&mut self) -> Option<CleanupState> {
        if !self
            .reader
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            return None;
        }
        let reader = self.reader.take().expect("finished PTY reader");
        // `is_finished` is polled only within `TeardownDeadline`; joining here
        // cannot wait for a still-running reader that retains the PTY master.
        Some(if reader.join().is_ok() {
            CleanupState::Complete
        } else {
            CleanupState::Failed
        })
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        let cleanup = self.terminate();
        #[cfg(test)]
        if let Some(success) = &self.drop_success {
            success.store(cleanup.successful(), Ordering::Release);
        }
    }
}

pub(super) fn assert_terminal_restored(output: &[u8]) {
    for sequence in [b"\x1b[?1049h".as_slice(), b"\x1b[?1049l".as_slice()] {
        assert!(
            output
                .windows(sequence.len())
                .any(|window| window == sequence),
            "terminal output omitted restoration sequence {sequence:?}: {}",
            String::from_utf8_lossy(output)
        );
    }
}

fn read_pty(mut reader: Box<dyn Read + Send>, output: &Mutex<Vec<u8>>) {
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => append_bounded(output, &buffer[..read]),
        }
    }
}

fn append_bounded(output: &Mutex<Vec<u8>>, bytes: &[u8]) {
    let mut output = output.lock().expect("PTY output lock");
    let overflow = output
        .len()
        .saturating_add(bytes.len())
        .saturating_sub(OUTPUT_LIMIT);
    if overflow > 0 {
        let remove = overflow.min(output.len());
        output.drain(..remove);
    }
    output.extend_from_slice(bytes);
}

fn child_pid(child: &(dyn Child + Send + Sync)) -> Option<Pid> {
    child
        .process_id()
        .and_then(|process| i32::try_from(process).ok())
        .and_then(Pid::from_raw)
}

fn owner_is_ready(product: &InstalledProduct, session: &str) -> bool {
    let directory = product.state.join("runtime/instances");
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("json")
        })
        .any(|entry| {
            fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|metadata| {
                    metadata["session_id"] == session
                        && metadata["control_protocol"].as_u64()
                            == Some(u64::from(proqi::ports::control::CONTROL_PROTOCOL_VERSION))
                        && metadata["control_endpoint"]
                            .as_str()
                            .is_some_and(|endpoint| std::path::Path::new(endpoint).exists())
                })
        })
}

#[cfg(test)]
#[path = "process/tests.rs"]
mod tests;
