//! Companion records in Herdr's private plugin state directory.
//!
//! One exclusive lock serializes every toggle, so two rapid invocations cannot
//! both decide to open a companion. Records are bounded strict JSON written by
//! atomic rename. Unreadable state is treated as empty: losing a record only
//! means a dead pane is not replaced automatically, never that a pane is closed.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use fs4::{FileExt, TryLockError};
use serde::{Deserialize, Serialize};

use crate::{
    domain::SessionId,
    ports::companion::{CompanionError, CompanionRecord, CompanionRecords},
};

const STATE_FILE: &str = "companions.json";
const LOCK_FILE: &str = "companions.lock";
const STATE_VERSION: u32 = 1;
const MAX_STATE_BYTES: u64 = 256 * 1024;
/// Oldest tabs are forgotten first beyond this many records.
const MAX_RECORDS: usize = 512;
/// A waiting toggle outlasts the slowest complete toggle that holds the lock,
/// with margin for the derived bound's store allowances.
pub(super) const LOCK_TIMEOUT: Duration =
    super::TOGGLE_WORST_CASE.saturating_add(Duration::from_secs(30));

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StateDocument {
    version: u32,
    companions: Vec<RecordWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecordWire {
    #[serde(rename = "tab_id")]
    tab: String,
    #[serde(rename = "pane_id")]
    pane: Option<String>,
    #[serde(rename = "session_id")]
    session: SessionId,
}

/// Exclusive, file-backed companion records for one plugin installation.
pub struct FileCompanionRecords {
    directory: PathBuf,
    _lock: File,
}

impl FileCompanionRecords {
    /// Acquire the plugin-wide toggle lock inside `directory`.
    ///
    /// # Errors
    ///
    /// Returns [`CompanionError::State`] when the directory or lock is unavailable.
    pub fn acquire(directory: &Path) -> Result<Self, CompanionError> {
        Self::acquire_within(directory, LOCK_TIMEOUT)
    }

    pub(super) fn acquire_within(
        directory: &Path,
        timeout: Duration,
    ) -> Result<Self, CompanionError> {
        crate::adapters::filesystem::prepare_private_dir(directory).map_err(state_error)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(LOCK_FILE))
            .map_err(state_error)?;
        let deadline = Instant::now() + timeout;
        loop {
            match FileExt::try_lock(&lock) {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(CompanionError::State(
                        "another Proqi toggle is still running".to_owned(),
                    ));
                }
                Err(TryLockError::Error(error)) => return Err(state_error(error)),
            }
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            _lock: lock,
        })
    }

    fn read(&self) -> Vec<CompanionRecord> {
        let Ok(file) = File::open(self.directory.join(STATE_FILE)) else {
            return Vec::new();
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_STATE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_STATE_BYTES
        {
            return Vec::new();
        }
        match serde_json::from_slice::<StateDocument>(&bytes) {
            Ok(document) if document.version == STATE_VERSION => document
                .companions
                .into_iter()
                .map(|record| CompanionRecord {
                    tab_id: record.tab,
                    pane_id: record.pane,
                    session_id: record.session,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn write(&self, records: &[CompanionRecord]) -> Result<(), CompanionError> {
        let document = StateDocument {
            version: STATE_VERSION,
            companions: records
                .iter()
                .map(|record| RecordWire {
                    tab: record.tab_id.clone(),
                    pane: record.pane_id.clone(),
                    session: record.session_id,
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&document).map_err(state_error)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_STATE_BYTES {
            return Err(CompanionError::State(
                "companion state exceeds its size limit".to_owned(),
            ));
        }
        let temporary = self
            .directory
            .join(format!("{STATE_FILE}.{}.tmp", std::process::id()));
        let result = (|| {
            let mut file = File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, self.directory.join(STATE_FILE))
        })();
        if result.is_err() {
            let _cleanup = fs::remove_file(&temporary);
        }
        result.map_err(state_error)
    }
}

impl CompanionRecords for FileCompanionRecords {
    fn all(&mut self) -> Result<Vec<CompanionRecord>, CompanionError> {
        Ok(self.read())
    }

    fn save(&mut self, record: &CompanionRecord) -> Result<(), CompanionError> {
        let mut records = self.read();
        records.retain(|existing| existing.tab_id != record.tab_id);
        records.push(record.clone());
        let excess = records.len().saturating_sub(MAX_RECORDS);
        records.drain(..excess);
        self.write(&records)
    }
}

fn state_error(error: impl std::fmt::Display) -> CompanionError {
    CompanionError::State(format!("Proqi plugin state is unavailable: {error}"))
}
