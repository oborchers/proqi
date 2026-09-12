//! Installation-wide durable session Browser history.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{BrowserMutation, BrowserOperation, OperationId, Timestamp},
    ports::store::{BrowserCommitReceipt, BrowserHistoryEntry, BrowserHistoryStatus, StoreError},
};

use super::{
    search::rebuild_session_search,
    support::{i64_to_usize, map_sql_error, operation_id_from_blob, usize_to_i64},
};

mod codec;
mod prune;

use codec::{decode, encode, kind_str, parse_kind};
pub(super) use prune::{invalidate_activity_conflicts, remove_session};

pub(super) fn commit(
    transaction: &Transaction<'_>,
    operation: &BrowserOperation,
) -> Result<BrowserCommitReceipt, StoreError> {
    operation
        .validate()
        .map_err(|error| StoreError::Invariant(error.to_string()))?;
    let payload = encode(operation)?;
    if let Some(receipt) = existing_operation(transaction, operation.id(), &payload)? {
        return Ok(receipt);
    }
    ensure_commit_id_unused(transaction, operation.id())?;
    let (cursor, count) = cursor_and_count(transaction)?;
    transaction
        .execute(
            "DELETE FROM browser_operations WHERE history_index >= ?1",
            [usize_to_i64(cursor)?],
        )
        .map_err(map_sql_error)?;
    apply_mutation(transaction, operation.forward())?;
    transaction
        .execute(
            "INSERT INTO browser_operations(
                id, history_index, kind, target_session_id, payload_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                operation.id().database_bytes().as_slice(),
                usize_to_i64(cursor)?,
                kind_str(operation.kind()),
                operation.session_id().database_bytes().as_slice(),
                payload,
                operation.created_at().as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    let next_cursor = cursor.saturating_add(1);
    transaction
        .execute(
            "INSERT INTO browser_operation_receipts(
                id, target_session_id, payload_json, cursor, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                operation.id().database_bytes().as_slice(),
                operation.session_id().database_bytes().as_slice(),
                payload,
                usize_to_i64(next_cursor)?,
                operation.created_at().as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    set_cursor(transaction, next_cursor)?;
    debug_assert!(cursor <= count);
    Ok(BrowserCommitReceipt {
        operation_id: operation.id(),
        cursor: next_cursor,
        idempotent_replay: false,
    })
}

pub(super) fn move_history(
    transaction: &Transaction<'_>,
    request_id: OperationId,
    target: BrowserHistoryEntry,
    undo: bool,
    at: Timestamp,
) -> Result<BrowserCommitReceipt, StoreError> {
    if let Some(receipt) = existing_move(transaction, request_id, target.operation_id, undo)? {
        return Ok(receipt);
    }
    ensure_request_id_unused(transaction, request_id)?;
    let (cursor, count) = cursor_and_count(transaction)?;
    let index = if undo {
        cursor.checked_sub(1)
    } else {
        (cursor < count).then_some(cursor)
    }
    .ok_or_else(|| StoreError::Conflict(no_history_message(undo).to_owned()))?;
    let operation = load_operation(transaction, index)?;
    if history_entry(&operation) != target {
        return Err(StoreError::Conflict(
            "Browser history changed before the requested movement".to_owned(),
        ));
    }
    let mutation = if undo {
        operation.inverse()
    } else {
        operation.forward()
    };
    apply_mutation(transaction, mutation)?;
    let next_cursor = if undo {
        cursor.saturating_sub(1)
    } else {
        cursor.saturating_add(1)
    };
    set_cursor(transaction, next_cursor)?;
    transaction
        .execute(
            "INSERT INTO browser_history_receipts(
                id, target_operation_id, undo, cursor, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                request_id.database_bytes().as_slice(),
                operation.id().database_bytes().as_slice(),
                undo,
                usize_to_i64(next_cursor)?,
                at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    Ok(BrowserCommitReceipt {
        operation_id: request_id,
        cursor: next_cursor,
        idempotent_replay: false,
    })
}

pub(super) fn status(transaction: &Transaction<'_>) -> Result<BrowserHistoryStatus, StoreError> {
    let (cursor, count) = cursor_and_count(transaction)?;
    Ok(BrowserHistoryStatus {
        undo: cursor
            .checked_sub(1)
            .map(|index| load_entry(transaction, index))
            .transpose()?,
        redo: (cursor < count)
            .then(|| load_entry(transaction, cursor))
            .transpose()?,
    })
}

pub(super) fn operation(
    transaction: &Transaction<'_>,
    id: OperationId,
) -> Result<Option<BrowserOperation>, StoreError> {
    let stored: Option<(Vec<u8>, String)> = transaction
        .query_row(
            "SELECT target_session_id, payload_json
             FROM browser_operation_receipts WHERE id = ?1",
            [id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let operation = stored
        .map(|(target, payload)| {
            let operation = decode(&payload)?;
            if operation.id() != id
                || operation.session_id().database_bytes().as_slice() != target.as_slice()
            {
                return Err(StoreError::Corrupt(
                    "Browser operation receipt metadata does not match its payload".to_owned(),
                ));
            }
            Ok(operation)
        })
        .transpose()?;
    if operation.is_none() {
        ensure_commit_id_unused(transaction, id)?;
    }
    Ok(operation)
}

fn apply_mutation(
    transaction: &Transaction<'_>,
    mutation: &BrowserMutation,
) -> Result<(), StoreError> {
    match mutation {
        BrowserMutation::SetName {
            session_id,
            expected,
            value,
        } => {
            if value.as_ref().is_some_and(|name| name.trim().is_empty()) {
                return Err(StoreError::Invariant(
                    "session name cannot be blank".to_owned(),
                ));
            }
            let current: Option<Option<String>> = transaction
                .query_row(
                    "SELECT name FROM sessions WHERE id = ?1",
                    [session_id.database_bytes().as_slice()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_sql_error)?;
            let Some(current) = current else {
                return Err(StoreError::NotFound(session_id.to_string()));
            };
            if &current != expected {
                return Err(StoreError::Conflict(
                    "session name changed before Browser history commit".to_owned(),
                ));
            }
            transaction
                .execute(
                    "UPDATE sessions SET name = ?2 WHERE id = ?1",
                    params![session_id.database_bytes().as_slice(), value],
                )
                .map_err(map_sql_error)?;
            rebuild_session_search(transaction, *session_id)
        }
        BrowserMutation::SetDeletedAt {
            session_id,
            expected,
            value,
            expected_last_active_at,
            last_active_at,
        } => {
            let current: Option<(Option<i64>, i64)> = transaction
                .query_row(
                    "SELECT deleted_at, last_active_at FROM sessions WHERE id = ?1",
                    [session_id.database_bytes().as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(map_sql_error)?;
            let Some(current) = current else {
                return Err(StoreError::NotFound(session_id.to_string()));
            };
            if current.0.map(Timestamp::from_millis) != *expected
                || Timestamp::from_millis(current.1) != *expected_last_active_at
            {
                return Err(StoreError::Conflict(
                    "session trash metadata changed before Browser history commit".to_owned(),
                ));
            }
            transaction
                .execute(
                    "UPDATE sessions SET deleted_at = ?2, last_active_at = ?3 WHERE id = ?1",
                    params![
                        session_id.database_bytes().as_slice(),
                        value.map(Timestamp::as_millis),
                        last_active_at.as_millis(),
                    ],
                )
                .map_err(map_sql_error)?;
            rebuild_session_search(transaction, *session_id)
        }
    }
}

fn cursor_and_count(transaction: &Transaction<'_>) -> Result<(usize, usize), StoreError> {
    let cursor: i64 = transaction
        .query_row(
            "SELECT cursor FROM browser_history_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let count: i64 = transaction
        .query_row("SELECT count(*) FROM browser_operations", [], |row| {
            row.get(0)
        })
        .map_err(map_sql_error)?;
    let cursor = i64_to_usize(cursor)?;
    let count = i64_to_usize(count)?;
    if cursor > count {
        return Err(StoreError::Corrupt(
            "Browser history cursor exceeds retained operations".to_owned(),
        ));
    }
    Ok((cursor, count))
}

fn set_cursor(transaction: &Transaction<'_>, cursor: usize) -> Result<(), StoreError> {
    transaction
        .execute(
            "UPDATE browser_history_state SET cursor = ?1 WHERE singleton = 1",
            [usize_to_i64(cursor)?],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

fn load_operation(
    transaction: &Transaction<'_>,
    index: usize,
) -> Result<BrowserOperation, StoreError> {
    let (id, kind, target, payload): (Vec<u8>, String, Vec<u8>, String) = transaction
        .query_row(
            "SELECT id, kind, target_session_id, payload_json
             FROM browser_operations WHERE history_index = ?1",
            [usize_to_i64(index)?],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(map_sql_error)?;
    let operation = decode(&payload)?;
    if operation.id() != operation_id_from_blob(id)?
        || operation.kind() != parse_kind(&kind)?
        || operation.session_id().database_bytes().as_slice() != target.as_slice()
    {
        return Err(StoreError::Corrupt(
            "Browser operation metadata does not match its payload".to_owned(),
        ));
    }
    Ok(operation)
}

fn load_entry(
    transaction: &Transaction<'_>,
    index: usize,
) -> Result<BrowserHistoryEntry, StoreError> {
    Ok(history_entry(&load_operation(transaction, index)?))
}

fn history_entry(operation: &BrowserOperation) -> BrowserHistoryEntry {
    BrowserHistoryEntry {
        operation_id: operation.id(),
        session_id: operation.session_id(),
        kind: operation.kind(),
    }
}

fn existing_operation(
    transaction: &Transaction<'_>,
    id: OperationId,
    payload: &str,
) -> Result<Option<BrowserCommitReceipt>, StoreError> {
    let existing: Option<(String, i64)> = transaction
        .query_row(
            "SELECT payload_json, cursor FROM browser_operation_receipts WHERE id = ?1",
            [id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some((existing, cursor)) = existing else {
        return Ok(None);
    };
    if existing != payload {
        return Err(StoreError::Conflict(
            "Browser operation identity was reused for different content".to_owned(),
        ));
    }
    Ok(Some(BrowserCommitReceipt {
        operation_id: id,
        cursor: i64_to_usize(cursor)?,
        idempotent_replay: true,
    }))
}

fn existing_move(
    transaction: &Transaction<'_>,
    id: OperationId,
    target_operation_id: OperationId,
    undo: bool,
) -> Result<Option<BrowserCommitReceipt>, StoreError> {
    let existing: Option<(Vec<u8>, bool, i64)> = transaction
        .query_row(
            "SELECT target_operation_id, undo, cursor
             FROM browser_history_receipts WHERE id = ?1",
            [id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some((target, prior_undo, cursor)) = existing else {
        return Ok(None);
    };
    let target = operation_id_from_blob(target)?;
    if prior_undo != undo || target != target_operation_id {
        return Err(StoreError::Conflict(
            "Browser history request identity was reused".to_owned(),
        ));
    }
    Ok(Some(BrowserCommitReceipt {
        operation_id: id,
        cursor: i64_to_usize(cursor)?,
        idempotent_replay: true,
    }))
}

fn ensure_not_used_by_session_history(
    transaction: &Transaction<'_>,
    id: OperationId,
) -> Result<(), StoreError> {
    let used: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM commit_receipts WHERE external_id = ?1)",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if used {
        Err(StoreError::Conflict(
            "operation identity is already used by session history".to_owned(),
        ))
    } else {
        Ok(())
    }
}

pub(in crate::adapters::sqlite) fn ensure_not_used_by_browser_history(
    transaction: &Transaction<'_>,
    external_id: [u8; 16],
) -> Result<(), StoreError> {
    let used: bool = transaction
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM browser_operation_receipts WHERE id = ?1
                 UNION ALL
                 SELECT 1 FROM browser_history_receipts WHERE id = ?1
             )",
            [external_id.as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if used {
        Err(StoreError::Conflict(
            "durable identity is already used by Browser history".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn ensure_request_id_unused(
    transaction: &Transaction<'_>,
    id: OperationId,
) -> Result<(), StoreError> {
    let used: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM browser_operation_receipts WHERE id = ?1)",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if used {
        return Err(StoreError::Conflict(
            "operation identity is already used by Browser history".to_owned(),
        ));
    }
    ensure_not_used_by_session_history(transaction, id)
}

fn ensure_commit_id_unused(
    transaction: &Transaction<'_>,
    id: OperationId,
) -> Result<(), StoreError> {
    let used: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM browser_history_receipts WHERE id = ?1)",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if used {
        return Err(StoreError::Conflict(
            "operation identity is already used by Browser history".to_owned(),
        ));
    }
    ensure_not_used_by_session_history(transaction, id)
}

const fn no_history_message(undo: bool) -> &'static str {
    if undo {
        "nothing to undo in Browser history"
    } else {
        "nothing to redo in Browser history"
    }
}
