//! Read-only SQLite and backup checks.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
};

use serde_json::json;

use crate::{
    adapters::sqlite::{SqliteHealth, inspect_read_only_snapshot},
    ports::store::{STORAGE_PROTOCOL_VERSION, SUPPORTED_SCHEMA_VERSION},
};

use super::{
    DoctorCheck, DoctorCheckResult, DoctorStatus, metadata_if_present, private_mode, result, timed,
};

static SNAPSHOT_SEQUENCE: AtomicU32 = AtomicU32::new(0);

pub(super) fn check_database(data_dir: &Path) -> DoctorCheck {
    timed("sqlite", "storage", || {
        let source = data_dir.join("proqi.sqlite3");
        let Some(metadata) = metadata_if_present(&source) else {
            return result(
                DoctorStatus::Ok,
                "database is not initialized",
                json!({"present": false}),
                None,
            );
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() || !private_mode(&metadata) {
            return result(
                DoctorStatus::Fail,
                "database path is unsafe",
                json!({"present": true}),
                Some("Restore the database as a private regular file."),
            );
        }
        match DatabaseSnapshot::copy_from(&source)
            .and_then(|snapshot| inspect_read_only_snapshot(snapshot.path()))
        {
            Ok(snapshot) => database_result(&snapshot),
            Err(message) => result(
                DoctorStatus::Warning,
                "database could not be inspected read-only",
                json!({"present": true}),
                Some(&message),
            ),
        }
    })
}

fn database_result(facts: &SqliteHealth) -> DoctorCheckResult {
    let values = json!({"present": true, "schema_version": facts.schema, "storage_protocol": facts.protocol, "journal_mode": facts.journal, "synchronous": facts.synchronous, "quick_check": facts.integrity});
    if facts.integrity != "ok" {
        return result(
            DoctorStatus::Fail,
            "SQLite quick_check failed",
            values,
            Some("Preserve the database and collect diagnostics before attempting recovery."),
        );
    }
    if facts.schema > SUPPORTED_SCHEMA_VERSION || facts.protocol > STORAGE_PROTOCOL_VERSION {
        return result(
            DoctorStatus::Fail,
            "database is newer than this Proqi binary",
            values,
            Some("Use the Proqi version that last opened this database or update Proqi."),
        );
    }
    if facts.schema < SUPPORTED_SCHEMA_VERSION || facts.protocol < STORAGE_PROTOCOL_VERSION {
        return result(
            DoctorStatus::Warning,
            "database requires a supported migration",
            values,
            Some("Launch Proqi normally when no older Proqi process is active."),
        );
    }
    if !facts.journal.eq_ignore_ascii_case("wal") || facts.synchronous != 2 {
        return result(
            DoctorStatus::Fail,
            "SQLite durability pragmas do not match the contract",
            values,
            Some("Collect diagnostics before reopening the database."),
        );
    }
    result(
        DoctorStatus::Ok,
        "SQLite integrity and durability settings are valid",
        values,
        None,
    )
}

pub(super) fn check_backups(data_dir: &Path) -> DoctorCheck {
    timed("backups", "storage", || {
        let directory = data_dir.join("backups");
        let Some(metadata) = metadata_if_present(&directory) else {
            return result(
                DoctorStatus::Ok,
                "no migration backups are present",
                json!({"count": 0}),
                None,
            );
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() || !private_mode(&metadata) {
            return result(
                DoctorStatus::Fail,
                "backup directory is unsafe",
                json!({}),
                Some("Restore backups to a private regular directory."),
            );
        }
        let count = fs::read_dir(directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .count();
        result(
            DoctorStatus::Ok,
            "migration backup directory is readable",
            json!({"count": count}),
            None,
        )
    })
}

struct DatabaseSnapshot {
    directory: PathBuf,
    database: PathBuf,
}

impl DatabaseSnapshot {
    fn copy_from(source: &Path) -> Result<Self, String> {
        let sequence = SNAPSHOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("proqi-doctor-{}-{sequence}", std::process::id()));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let database = directory.join("proqi.sqlite3");
        if let Err(error) = copy_database_files(source, &database) {
            let _ = fs::remove_dir_all(&directory);
            return Err(error.to_string());
        }
        Ok(Self {
            directory,
            database,
        })
    }

    fn path(&self) -> &Path {
        &self.database
    }
}

impl Drop for DatabaseSnapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn copy_database_files(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::copy(source, destination)?;
    for suffix in ["-wal", "-shm"] {
        let source_sidecar = PathBuf::from(format!("{}{suffix}", source.display()));
        if source_sidecar.is_file() {
            fs::copy(
                source_sidecar,
                PathBuf::from(format!("{}{suffix}", destination.display())),
            )?;
        }
    }
    Ok(())
}
