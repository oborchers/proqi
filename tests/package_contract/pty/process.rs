use std::{
    io::Read,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use super::PtyChild;

pub(super) fn read_pty(mut reader: Box<dyn Read + Send>, output: &Mutex<Vec<u8>>) {
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

pub(super) fn finish(owner: PtyChild, timeout: Duration) -> Vec<u8> {
    let (success, output) = finish_with_status(owner, timeout);
    assert!(
        success,
        "PTY owner exited unsuccessfully: {}",
        String::from_utf8_lossy(&output)
    );
    output
}

pub(super) fn finish_with_status(mut owner: PtyChild, timeout: Duration) -> (bool, Vec<u8>) {
    let deadline = Instant::now() + timeout;
    loop {
        match owner.child.try_wait().expect("poll PTY child") {
            Some(status) => {
                let success = status.success();
                drop(owner.input);
                owner.reader.join().expect("join PTY reader");
                let output = owner.output.lock().expect("PTY output lock").clone();
                return (success, output);
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            None => {
                owner.child.kill().expect("kill timed-out PTY owner");
                panic!("PTY owner did not exit within {timeout:?}");
            }
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
