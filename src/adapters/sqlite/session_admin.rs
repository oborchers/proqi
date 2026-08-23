//! Atomic session metadata changes.

use std::path::Path;

use rusqlite::{Transaction, params};

use crate::{
    domain::{SessionId, Timestamp},
    ports::store::StoreError,
};

use super::{
    search::rebuild_session_search,
    support::{map_sql_error, path_to_bytes},
};

pub(super) fn record_open(
    transaction: &Transaction<'_>,
    id: SessionId,
    cwd: &Path,
    at: Timestamp,
) -> Result<(), StoreError> {
    if !cwd.is_absolute() {
        return Err(StoreError::Invariant(
            "session directory must be absolute".to_owned(),
        ));
    }
    let changed = transaction
        .execute(
            "UPDATE sessions SET last_opened_cwd = ?2, last_opened_at = ?3,
             last_active_at = max(last_active_at, ?3) WHERE id = ?1 AND deleted_at IS NULL",
            params![
                id.database_bytes().as_slice(),
                path_to_bytes(cwd),
                at.as_millis()
            ],
        )
        .map_err(map_sql_error)?;
    require_changed(changed, id)?;
    rebuild_session_search(transaction, id)
}

pub(super) fn rename(
    transaction: &Transaction<'_>,
    id: SessionId,
    name: Option<&str>,
) -> Result<(), StoreError> {
    if name.is_some_and(|value| value.trim().is_empty()) {
        return Err(StoreError::Invariant(
            "session name cannot be blank".to_owned(),
        ));
    }
    let changed = transaction
        .execute(
            "UPDATE sessions SET name = ?2 WHERE id = ?1",
            params![id.database_bytes().as_slice(), name],
        )
        .map_err(map_sql_error)?;
    require_changed(changed, id)?;
    rebuild_session_search(transaction, id)
}

fn require_changed(changed: usize, id: SessionId) -> Result<(), StoreError> {
    if changed == 0 {
        Err(StoreError::NotFound(id.to_string()))
    } else {
        Ok(())
    }
}
