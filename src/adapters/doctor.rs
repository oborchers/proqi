//! Read-only, content-redacted installation health checks.

mod environment;
mod storage;

use std::{fmt, fs, path::Path, time::Instant};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{ports::environment::AppPaths, ui::UiSettings};

const WARNING_DISK_BYTES: u64 = 256 * 1024 * 1024;
const FAILURE_DISK_BYTES: u64 = 32 * 1024 * 1024;
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// Complete versioned result from `proqi doctor`.
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    /// Doctor contract version.
    pub schema_version: u32,
    /// Running Proqi version.
    pub proqi_version: &'static str,
    /// Highest-severity result.
    pub overall_status: DoctorStatus,
    /// Stable ordered checks.
    pub checks: Vec<DoctorCheck>,
}

/// Stable health classification.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    /// Check passed.
    Ok,
    /// Optional capability or state was not applicable.
    Skipped,
    /// Attention is useful but ordinary standalone operation remains safe.
    Warning,
    /// A confirmed condition makes operation unsafe or unreliable.
    Fail,
}

impl fmt::Display for DoctorStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ok => "ok",
            Self::Skipped => "skipped",
            Self::Warning => "warning",
            Self::Fail => "fail",
        })
    }
}

/// One stable, content-redacted health check.
#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    /// Stable support identifier.
    pub id: &'static str,
    /// Diagnostic category.
    pub category: &'static str,
    /// Result classification.
    pub status: DoctorStatus,
    /// Content-free explanation.
    pub summary: String,
    /// Safe structured observations.
    pub facts: Value,
    /// Optional next action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Runtime of this check.
    pub duration_ms: u64,
}

/// Inspect local state without repairing or creating canonical Proqi data.
#[must_use]
pub fn inspect(paths: &AppPaths) -> DoctorReport {
    let checks = vec![
        check_paths(paths),
        check_config(&paths.config_dir),
        check_disk(&paths.data_dir),
        storage::check_database(&paths.data_dir),
        storage::check_backups(&paths.data_dir),
        environment::check_runtime(&paths.runtime_dir),
        environment::check_update_cache(&paths.cache_dir),
        environment::check_terminal(),
        environment::check_herdr(),
    ];
    let overall_status = checks
        .iter()
        .map(|check| check.status)
        .max()
        .unwrap_or(DoctorStatus::Ok);
    DoctorReport {
        schema_version: 1,
        proqi_version: env!("CARGO_PKG_VERSION"),
        overall_status,
        checks,
    }
}

fn check_paths(paths: &AppPaths) -> DoctorCheck {
    timed("data_paths", "filesystem", || {
        let entries = [
            ("data", &paths.data_dir),
            ("config", &paths.config_dir),
            ("cache", &paths.cache_dir),
            ("runtime", &paths.runtime_dir),
        ];
        let unsafe_entries = entries
            .iter()
            .filter_map(|(name, path)| unsafe_path(path).then_some(*name))
            .collect::<Vec<_>>();
        if unsafe_entries.is_empty() {
            result(
                DoctorStatus::Ok,
                "state paths are private or not initialized",
                json!({"checked": 4}),
                None,
            )
        } else {
            result(
                DoctorStatus::Fail,
                "one or more state paths are unsafe",
                json!({"unsafe": unsafe_entries}),
                Some("Replace symlinks or non-private state directories before running Proqi."),
            )
        }
    })
}

fn check_config(config_dir: &Path) -> DoctorCheck {
    timed("config", "configuration", || {
        let path = config_dir.join("config.toml");
        let Some(metadata) = metadata_if_present(&path) else {
            return result(
                DoctorStatus::Ok,
                "default configuration is in use",
                json!({"present": false}),
                None,
            );
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_CONFIG_BYTES
            || !private_mode(&metadata)
        {
            return result(
                DoctorStatus::Fail,
                "configuration file is unsafe",
                json!({"present": true}),
                Some("Use a private regular config.toml no larger than 64 KiB."),
            );
        }
        let valid = fs::read_to_string(&path)
            .ok()
            .and_then(|value| toml::from_str::<UiSettings>(&value).ok())
            .is_some_and(|settings| settings.keybindings.validate().is_ok());
        if valid {
            result(
                DoctorStatus::Ok,
                "configuration is valid",
                json!({"present": true}),
                None,
            )
        } else {
            result(
                DoctorStatus::Fail,
                "configuration cannot be parsed safely",
                json!({"present": true}),
                Some("Correct config.toml or move it aside and retry."),
            )
        }
    })
}

fn check_disk(data_dir: &Path) -> DoctorCheck {
    timed("disk_space", "filesystem", || {
        let Some(existing) = data_dir.ancestors().find(|candidate| candidate.exists()) else {
            return result(
                DoctorStatus::Warning,
                "available disk space could not be determined",
                json!({}),
                None,
            );
        };
        match fs2::available_space(existing) {
            Ok(bytes) if bytes < FAILURE_DISK_BYTES => result(
                DoctorStatus::Fail,
                "available disk space is critically low",
                json!({"available_bytes": bytes}),
                Some("Free at least 256 MiB before editing durable thoughts."),
            ),
            Ok(bytes) if bytes < WARNING_DISK_BYTES => result(
                DoctorStatus::Warning,
                "available disk space is low",
                json!({"available_bytes": bytes}),
                Some("Free at least 256 MiB to protect autosave and backups."),
            ),
            Ok(bytes) => result(
                DoctorStatus::Ok,
                "available disk space is sufficient",
                json!({"available_bytes": bytes}),
                None,
            ),
            Err(_) => result(
                DoctorStatus::Warning,
                "available disk space could not be determined",
                json!({}),
                None,
            ),
        }
    })
}

pub(super) type DoctorCheckResult = (DoctorStatus, String, Value, Option<String>);

pub(super) fn timed(
    id: &'static str,
    category: &'static str,
    check: impl FnOnce() -> DoctorCheckResult,
) -> DoctorCheck {
    let started = Instant::now();
    let (status, summary, facts, remediation) = check();
    DoctorCheck {
        id,
        category,
        status,
        summary,
        facts,
        remediation,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }
}

pub(super) fn result(
    status: DoctorStatus,
    summary: &str,
    facts: Value,
    remediation: Option<&str>,
) -> DoctorCheckResult {
    (
        status,
        summary.to_owned(),
        facts,
        remediation.map(str::to_owned),
    )
}

pub(super) fn metadata_if_present(path: &Path) -> Option<fs::Metadata> {
    fs::symlink_metadata(path).ok()
}

fn unsafe_path(path: &Path) -> bool {
    metadata_if_present(path).is_some_and(|metadata| {
        metadata.file_type().is_symlink() || !metadata.is_dir() || !private_mode(&metadata)
    })
}

#[cfg(unix)]
pub(super) fn private_mode(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode().trailing_zeros() >= 6
}

#[cfg(not(unix))]
pub(super) fn private_mode(_: &fs::Metadata) -> bool {
    true
}
