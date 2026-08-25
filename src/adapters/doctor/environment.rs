//! Read-only runtime, update, terminal, and integration checks.

use std::{
    fs,
    io::IsTerminal as _,
    path::{Path, PathBuf},
};

use fs4::TryLockError;
use serde_json::json;

use crate::{
    domain::UpdateCacheState,
    ports::{agent::AgentGateway as _, runtime::InstanceInfo},
};

use super::{DoctorCheck, DoctorStatus, private_mode, result, timed};

const MAX_UPDATE_STATE_BYTES: u64 = 16 * 1024;

pub(super) fn check_runtime(runtime_dir: &Path) -> DoctorCheck {
    timed("runtime_metadata", "runtime", || {
        let directory = runtime_dir.join("instances");
        if !directory.is_dir() {
            return result(
                DoctorStatus::Ok,
                "runtime metadata is not initialized",
                json!({"active": 0, "stale": 0, "malformed": 0}),
                None,
            );
        }
        let mut active = 0_u64;
        let mut stale = 0_u64;
        let mut malformed = 0_u64;
        for path in regular_files(&directory) {
            match fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<InstanceInfo>(&bytes).ok())
            {
                Some(info) if lock_is_active(runtime_dir, &info) => active += 1,
                Some(_) => stale += 1,
                None => malformed += 1,
            }
        }
        let facts = json!({"active": active, "stale": stale, "malformed": malformed});
        if malformed > 0 {
            result(
                DoctorStatus::Warning,
                "malformed runtime metadata is present",
                facts,
                Some(
                    "Exit Proqi instances, then relaunch to allow normal stale-metadata recovery.",
                ),
            )
        } else if stale > 0 {
            result(
                DoctorStatus::Warning,
                "stale runtime metadata is present",
                facts,
                Some("A normal Proqi runtime scan will remove verified stale metadata."),
            )
        } else {
            result(
                DoctorStatus::Ok,
                "runtime metadata is consistent",
                facts,
                None,
            )
        }
    })
}

pub(super) fn check_update_cache(cache_dir: &Path) -> DoctorCheck {
    timed("update_cache", "updates", || {
        let root = cache_dir.join("updates");
        if !root.is_dir() {
            return result(
                DoctorStatus::Ok,
                "update cache is not initialized",
                json!({"entries": 0}),
                None,
            );
        }
        let mut valid = 0_u64;
        let mut invalid = 0_u64;
        for path in state_files(&root) {
            if valid_update_state(&path) {
                valid += 1;
            } else {
                invalid += 1;
            }
        }
        let facts = json!({"entries": valid + invalid, "invalid": invalid});
        if invalid == 0 {
            result(DoctorStatus::Ok, "update cache is valid", facts, None)
        } else {
            result(
                DoctorStatus::Warning,
                "invalid update cache entries are present",
                facts,
                Some("Proqi will ignore invalid update cache data safely."),
            )
        }
    })
}

fn valid_update_state(path: &Path) -> bool {
    let metadata = fs::symlink_metadata(path).ok();
    metadata.as_ref().is_some_and(|value| {
        value.is_file()
            && !value.file_type().is_symlink()
            && value.len() <= MAX_UPDATE_STATE_BYTES
            && private_mode(value)
    }) && fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UpdateCacheState>(&bytes).ok())
        .is_some()
}

pub(super) fn check_terminal() -> DoctorCheck {
    timed("terminal", "terminal", || {
        let stdin = std::io::stdin().is_terminal();
        let stdout = std::io::stdout().is_terminal();
        let facts = json!({"stdin": stdin, "stdout": stdout});
        if stdin && stdout {
            result(
                DoctorStatus::Ok,
                "interactive terminal input and output are available",
                facts,
                None,
            )
        } else {
            result(
                DoctorStatus::Skipped,
                "doctor is running without an interactive terminal",
                facts,
                None,
            )
        }
    })
}

pub(super) fn check_herdr() -> DoctorCheck {
    timed("herdr", "integration", || {
        if std::env::var_os("HERDR_ENV").as_deref() != Some(std::ffi::OsStr::new("1")) {
            return result(
                DoctorStatus::Skipped,
                "Herdr is not managing this pane",
                json!({"managed": false}),
                None,
            );
        }
        let mut gateway =
            crate::adapters::herdr::HerdrGateway::from_environment("proqi-doctor".to_owned());
        match gateway.capabilities() {
            Ok(capabilities) => result(
                DoctorStatus::Ok,
                "Herdr semantic submission is compatible",
                json!({"managed": true, "provider": capabilities.provider, "protocol": capabilities.protocol, "version": capabilities.version}),
                None,
            ),
            Err(error) => result(
                DoctorStatus::Warning,
                "Herdr compatibility could not be verified",
                json!({"managed": true}),
                Some(&error.to_string()),
            ),
        }
    })
}

fn regular_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect()
}

fn state_files(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_dir(entry.path()).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_some_and(|name| name == "state.json"))
        .collect()
}

fn lock_is_active(runtime_dir: &Path, info: &InstanceInfo) -> bool {
    let path = runtime_dir
        .join("sessions")
        .join(format!("{}.lock", info.session_id));
    let Ok(file) = fs::OpenOptions::new().read(true).open(path) else {
        return false;
    };
    match fs4::FileExt::try_lock_shared(&file) {
        Ok(()) => {
            let _ = fs4::FileExt::unlock(&file);
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => false,
    }
}
