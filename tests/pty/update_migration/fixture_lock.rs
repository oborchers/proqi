//! Cross-process serialization for expensive historical executable fixtures.

use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use fs4::{FileExt, TryLockError};

const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const WAIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) struct FixtureLease {
    file: File,
}

impl Drop for FixtureLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(super) fn acquire() -> FixtureLease {
    let path = lock_path();
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap_or_else(|error| {
            panic!(
                "open cross-version fixture lock {}: {error}",
                path.display()
            )
        });
    let waiting_since = Instant::now();
    loop {
        match FileExt::try_lock(&file) {
            Ok(()) => return FixtureLease { file },
            Err(TryLockError::WouldBlock) => {
                assert!(
                    waiting_since.elapsed() < WAIT_TIMEOUT,
                    "cross-version fixture lock remained owned for {WAIT_TIMEOUT:?}"
                );
                thread::sleep(RETRY_INTERVAL);
            }
            Err(TryLockError::Error(error)) => {
                panic!(
                    "acquire cross-version fixture lock {}: {error}",
                    path.display()
                );
            }
        }
    }
}

fn lock_path() -> PathBuf {
    std::env::current_exe()
        .expect("current test executable")
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Cargo profile directory")
        .join("proqi-cross-version-fixture.lock")
}
