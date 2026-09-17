//! Opt-in integration-test evidence for owner-control queue admission.

use std::{fs, path::PathBuf};

pub(super) fn record_queued_request() {
    if std::env::var_os("PROQI_TEST_INPUT_STALL").is_none() {
        return;
    }
    let Some(path) = std::env::var_os("PROQI_TEST_CONTROL_QUEUED").map(PathBuf::from) else {
        return;
    };
    let result = fs::write(path, b"queued");
    debug_assert!(result.is_ok(), "test control-queue probe must be writable");
}
