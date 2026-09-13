//! Selective Browser-history cleanup for an irreversibly pruned session.

use rusqlite::{Transaction, params};

use crate::{domain::SessionId, ports::store::StoreError};

use super::super::support::{i64_to_usize, map_sql_error, usize_to_i64};

/// Remove one session's history without discarding unrelated Browser entries.
pub(in crate::adapters::sqlite) fn remove_session(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<(), StoreError> {
    let (cursor, count) = super::cursor_and_count(transaction)?;
    let removed_before: i64 = transaction
        .query_row(
            "SELECT count(*) FROM browser_operations
             WHERE target_session_id = ?1 AND history_index < ?2",
            params![
                session_id.database_bytes().as_slice(),
                usize_to_i64(cursor)?,
            ],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "DELETE FROM browser_history_receipts
             WHERE target_operation_id IN (
                 SELECT id FROM browser_operation_receipts WHERE target_session_id = ?1
             )",
            [session_id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "DELETE FROM browser_operation_receipts WHERE target_session_id = ?1",
            [session_id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "DELETE FROM browser_operations WHERE target_session_id = ?1",
            [session_id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;

    normalize_indices(transaction, count)?;
    let removed_before = i64_to_usize(removed_before)?;
    super::set_cursor(transaction, cursor.saturating_sub(removed_before))
}

/// Remove only deletion-state redo entries whose activity precondition changed.
pub(in crate::adapters::sqlite) fn invalidate_activity_conflicts(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<(), StoreError> {
    let (cursor, count) = super::cursor_and_count(transaction)?;
    let removed = transaction
        .execute(
            "DELETE FROM browser_operations
             WHERE target_session_id = ?1 AND history_index >= ?2
             AND kind IN ('trash', 'restore')",
            params![
                session_id.database_bytes().as_slice(),
                usize_to_i64(cursor)?,
            ],
        )
        .map_err(map_sql_error)?;
    if removed > 0 {
        normalize_indices(transaction, count)?;
    }
    Ok(())
}

fn normalize_indices(transaction: &Transaction<'_>, prior_count: usize) -> Result<(), StoreError> {
    let ids = {
        let mut statement = transaction
            .prepare("SELECT id FROM browser_operations ORDER BY history_index")
            .map_err(map_sql_error)?;
        statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sql_error)?
    };
    if ids.is_empty() {
        return Ok(());
    }
    let offset = prior_count.saturating_add(1);
    transaction
        .execute(
            "UPDATE browser_operations SET history_index = history_index + ?1",
            [usize_to_i64(offset)?],
        )
        .map_err(map_sql_error)?;
    for (index, id) in ids.iter().enumerate() {
        transaction
            .execute(
                "UPDATE browser_operations SET history_index = ?2 WHERE id = ?1",
                params![id, usize_to_i64(index)?],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}
