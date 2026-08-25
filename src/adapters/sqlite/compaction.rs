//! Transactional, lease-gated history compaction.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{BoardOperation, SessionId, ThoughtId, ThoughtRevision},
    ports::store::{CompactedOperationRequest, StoreError},
};

use super::{
    receipt_compaction::{board_replay, compact_receipt},
    support::{map_sql_error, thought_id_from_blob},
};

const BOARD_COUNT_LIMIT: usize = 500;
const BOARD_BYTES_LIMIT: usize = 16 * 1024 * 1024;
const EDITOR_COUNT_LIMIT: usize = 200;
const EDITOR_BYTES_LIMIT: usize = 8 * 1024 * 1024;
const EDITOR_SESSION_BYTES_LIMIT: usize = 48 * 1024 * 1024;

struct HistoryRow {
    id: [u8; 16],
    bytes: usize,
    payload: String,
}

pub(super) fn compact_session(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<(), StoreError> {
    let board_cursor: Option<i64> = transaction
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    let board_cursor = board_cursor.ok_or_else(|| StoreError::NotFound(session_id.to_string()))?;
    compact_board(transaction, session_id, board_cursor)?;
    let thoughts = editor_thoughts(transaction, session_id)?;
    for (thought_id, cursor) in &thoughts {
        compact_editor(transaction, *thought_id, *cursor)?;
    }
    compact_editor_aggregate(transaction, session_id, &thoughts)
}

fn compact_board(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    cursor: i64,
) -> Result<(), StoreError> {
    let rows = board_rows(transaction, session_id)?;
    let drop = drop_count(&rows, cursor, BOARD_COUNT_LIMIT, BOARD_BYTES_LIMIT)?;
    if drop == 0 {
        return Ok(());
    }
    for row in rows.iter().take(drop) {
        let operation: BoardOperation = serde_json::from_str(&row.payload)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        compact_receipt(transaction, "operation", row.id, board_replay(&operation)?)?;
    }
    delete_and_reindex_board(transaction, session_id, cursor, rows.len(), drop)
}

fn compact_editor(
    transaction: &Transaction<'_>,
    thought_id: ThoughtId,
    cursor: i64,
) -> Result<(), StoreError> {
    let rows = revision_rows(transaction, thought_id)?;
    let drop = drop_count(&rows, cursor, EDITOR_COUNT_LIMIT, EDITOR_BYTES_LIMIT)?;
    drop_editor_prefix(transaction, thought_id, cursor, &rows, drop)
}

fn compact_editor_aggregate(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    thoughts: &[(ThoughtId, i64)],
) -> Result<(), StoreError> {
    let total: i64 = transaction
        .query_row(
            "SELECT coalesce(sum(length(payload_json)), 0) FROM thought_revisions
             WHERE session_id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let limit = i64::try_from(EDITOR_SESSION_BYTES_LIMIT)
        .map_err(|_| StoreError::Corrupt("history limit overflow".to_owned()))?;
    if total <= limit {
        return Ok(());
    }
    let mut excess = usize::try_from(total - limit)
        .map_err(|_| StoreError::Corrupt("history size overflow".to_owned()))?;
    for (thought_id, cursor) in thoughts {
        if excess == 0 {
            break;
        }
        let rows = revision_rows(transaction, *thought_id)?;
        let safe = safe_prefix(*cursor, rows.len())?;
        let mut drop = 0;
        let mut removed = 0_usize;
        for row in rows.iter().take(safe) {
            drop += 1;
            removed = removed.saturating_add(row.bytes);
            if removed >= excess {
                break;
            }
        }
        drop_editor_prefix(transaction, *thought_id, *cursor, &rows, drop)?;
        excess = excess.saturating_sub(removed);
    }
    Ok(())
}

fn drop_editor_prefix(
    transaction: &Transaction<'_>,
    thought_id: ThoughtId,
    cursor: i64,
    rows: &[HistoryRow],
    drop: usize,
) -> Result<(), StoreError> {
    if drop == 0 {
        return Ok(());
    }
    for row in rows.iter().take(drop) {
        let _: ThoughtRevision = serde_json::from_str(&row.payload)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        compact_receipt(
            transaction,
            "revision",
            row.id,
            CompactedOperationRequest::Opaque,
        )?;
    }
    delete_and_reindex_editor(transaction, thought_id, cursor, rows.len(), drop)
}

fn drop_count(
    rows: &[HistoryRow],
    cursor: i64,
    count_limit: usize,
    byte_limit: usize,
) -> Result<usize, StoreError> {
    let safe = safe_prefix(cursor, rows.len())?;
    let mut count = rows.len();
    let mut bytes = rows.iter().map(|row| row.bytes).sum::<usize>();
    let mut drop = 0;
    while drop < safe && (count > count_limit || bytes > byte_limit) {
        bytes = bytes.saturating_sub(rows[drop].bytes);
        count -= 1;
        drop += 1;
    }
    Ok(drop)
}

fn safe_prefix(cursor: i64, rows: usize) -> Result<usize, StoreError> {
    let cursor = usize::try_from(cursor)
        .map_err(|_| StoreError::Corrupt("negative history cursor".to_owned()))?;
    if cursor > rows {
        return Err(StoreError::Corrupt(
            "history cursor exceeds retained rows".to_owned(),
        ));
    }
    Ok(cursor.saturating_sub(1))
}

fn board_rows(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<Vec<HistoryRow>, StoreError> {
    load_rows(
        transaction,
        "SELECT id, length(payload_json), payload_json FROM board_operations
         WHERE session_id = ?1 ORDER BY history_index",
        session_id.database_bytes().as_slice(),
    )
}

fn revision_rows(
    transaction: &Transaction<'_>,
    thought_id: ThoughtId,
) -> Result<Vec<HistoryRow>, StoreError> {
    load_rows(
        transaction,
        "SELECT id, length(payload_json), payload_json FROM thought_revisions
         WHERE thought_id = ?1 ORDER BY history_index",
        thought_id.database_bytes().as_slice(),
    )
}

fn load_rows(
    transaction: &Transaction<'_>,
    sql: &str,
    id: &[u8],
) -> Result<Vec<HistoryRow>, StoreError> {
    let mut statement = transaction.prepare(sql).map_err(map_sql_error)?;
    let records = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(map_sql_error)?;
    let mut rows = Vec::new();
    for record in records {
        let (id, bytes, payload) = record.map_err(map_sql_error)?;
        let id: [u8; 16] = id
            .try_into()
            .map_err(|_| StoreError::Corrupt("invalid history identifier".to_owned()))?;
        rows.push(HistoryRow {
            id,
            bytes: usize::try_from(bytes)
                .map_err(|_| StoreError::Corrupt("invalid history size".to_owned()))?,
            payload,
        });
    }
    Ok(rows)
}

fn editor_thoughts(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<Vec<(ThoughtId, i64)>, StoreError> {
    let mut statement = transaction
        .prepare("SELECT id, editor_history_cursor FROM thoughts WHERE session_id = ?1")
        .map_err(map_sql_error)?;
    let records = statement
        .query_map([session_id.database_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(map_sql_error)?;
    let mut thoughts = Vec::new();
    for record in records {
        let (id, cursor) = record.map_err(map_sql_error)?;
        thoughts.push((thought_id_from_blob(id)?, cursor));
    }
    Ok(thoughts)
}

fn delete_and_reindex_board(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    cursor: i64,
    total: usize,
    drop: usize,
) -> Result<(), StoreError> {
    let drop = i64::try_from(drop).map_err(|_| StoreError::Corrupt("drop overflow".to_owned()))?;
    let offset = i64::try_from(total + 1)
        .map_err(|_| StoreError::Corrupt("history offset overflow".to_owned()))?;
    transaction
        .execute(
            "DELETE FROM board_operations WHERE session_id = ?1 AND history_index < ?2",
            params![session_id.database_bytes().as_slice(), drop],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE board_operations SET history_index = history_index + ?2 WHERE session_id = ?1",
            params![session_id.database_bytes().as_slice(), offset],
        )
        .map_err(map_sql_error)?;
    transaction.execute("UPDATE board_operations SET history_index = history_index - ?2 - ?3 WHERE session_id = ?1", params![session_id.database_bytes().as_slice(), offset, drop]).map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE sessions SET board_history_cursor = ?2 WHERE id = ?1",
            params![session_id.database_bytes().as_slice(), cursor - drop],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

fn delete_and_reindex_editor(
    transaction: &Transaction<'_>,
    thought_id: ThoughtId,
    cursor: i64,
    total: usize,
    drop: usize,
) -> Result<(), StoreError> {
    let drop = i64::try_from(drop).map_err(|_| StoreError::Corrupt("drop overflow".to_owned()))?;
    let offset = i64::try_from(total + 1)
        .map_err(|_| StoreError::Corrupt("history offset overflow".to_owned()))?;
    transaction
        .execute(
            "DELETE FROM thought_revisions WHERE thought_id = ?1 AND history_index < ?2",
            params![thought_id.database_bytes().as_slice(), drop],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE thought_revisions SET history_index = history_index + ?2 WHERE thought_id = ?1",
            params![thought_id.database_bytes().as_slice(), offset],
        )
        .map_err(map_sql_error)?;
    transaction.execute("UPDATE thought_revisions SET history_index = history_index - ?2 - ?3 WHERE thought_id = ?1", params![thought_id.database_bytes().as_slice(), offset, drop]).map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE thoughts SET editor_history_cursor = ?2 WHERE id = ?1",
            params![thought_id.database_bytes().as_slice(), cursor - drop],
        )
        .map_err(map_sql_error)?;
    Ok(())
}
