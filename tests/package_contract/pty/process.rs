//! Panic-safe ownership of an installed product running under a Unix PTY.

use std::{
    fs,
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};
use serde_json::Value;

use crate::InstalledProduct;

pub(super) struct PtyChild {
    child: Option<Box<dyn Child + Send + Sync>>,
    input: Option<Box<dyn Write + Send>>,
    output: Arc<Mutex<Vec<u8>>>,
    reader: Option<thread::JoinHandle<()>>,
}

impl PtyChild {
    pub(super) fn spawn(product: &InstalledProduct, session: &str) -> Self {
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
        let output = Arc::new(Mutex::new(Vec::new()));
        let reader_output = Arc::clone(&output);
        let reader = thread::spawn(move || read_pty(reader, &reader_output));
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
        let child = pair
            .slave
            .spawn_command(command)
            .expect("spawn installed PTY owner");
        drop(pair.slave);
        Self {
            child: Some(child),
            input: Some(input),
            output,
            reader: Some(reader),
        }
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
            if let Some(status) = self
                .child
                .as_mut()
                .expect("live PTY child")
                .try_wait()
                .expect("poll starting owner")
            {
                self.child.take();
                self.close_io();
                panic!(
                    "installed owner exited with {status}: {}",
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
            match self
                .child
                .as_mut()
                .expect("live PTY child")
                .try_wait()
                .expect("poll PTY child")
            {
                Some(status) => {
                    self.child.take();
                    self.close_io();
                    return (status.success(), self.output());
                }
                None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                None => {
                    self.terminate();
                    panic!("PTY owner did not exit within {timeout:?}");
                }
            }
        }
    }

    fn output(&self) -> Vec<u8> {
        self.output.lock().expect("PTY output lock").clone()
    }

    fn close_io(&mut self) {
        self.input.take();
        if let Some(reader) = self.reader.take() {
            reader.join().expect("join PTY reader");
        }
    }

    fn terminate(&mut self) {
        self.input.take();
        if let Some(mut child) = self.child.take()
            && !matches!(child.try_wait(), Ok(Some(_)))
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        self.terminate();
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
            Ok(read) => output
                .lock()
                .expect("PTY output lock")
                .extend_from_slice(&buffer[..read]),
        }
    }
}

fn owner_is_ready(product: &InstalledProduct, session: &str) -> bool {
    let directory = product.state.join("runtime/instances");
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
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
