//! Atomic persistence of the canonical mixed-item Board projection.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{SessionBoard, Timestamp},
    ports::store::StoreError,
};

use super::support::{map_sql_error, usize_to_i64};

pub(super) fn persist_board(
    transaction: &Transaction<'_>,
    board: &SessionBoard,
) -> Result<(), StoreError> {
    super::attachment_numbering::persist(
        transaction,
        board.session.id,
        board.attachment_counters(),
    )?;
    shift_existing_positions(transaction, board)?;
    persist_thoughts(transaction, board)?;
    persist_separators(transaction, board)
}

fn shift_existing_positions(
    transaction: &Transaction<'_>,
    board: &SessionBoard,
) -> Result<(), StoreError> {
    let maximum: Option<i64> = transaction
        .query_row(
            "SELECT max(position) FROM (
                SELECT position FROM thoughts WHERE session_id = ?1 AND deleted_at IS NULL
                UNION ALL
                SELECT position FROM separators WHERE session_id = ?1 AND deleted_at IS NULL
             )",
            [board.session.id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let offset = maximum
        .unwrap_or(0)
        .checked_add(usize_to_i64(board.live_items().len())?)
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| StoreError::Corrupt("Board item position offset overflow".to_owned()))?;
    for table in ["thoughts", "separators"] {
        transaction
            .execute(
                &format!(
                    "UPDATE {table} SET position = position + ?2
                     WHERE session_id = ?1 AND deleted_at IS NULL"
                ),
                params![board.session.id.database_bytes().as_slice(), offset],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}

fn persist_thoughts(transaction: &Transaction<'_>, board: &SessionBoard) -> Result<(), StoreError> {
    for thought in board.thoughts() {
        let annotations_json = serde_json::to_string(&thought.annotations)
            .map_err(|error| StoreError::Serialization(error.to_string()))?;
        let cursor: i64 = transaction
            .query_row(
                "SELECT editor_history_cursor FROM thoughts WHERE id = ?1",
                [thought.id.database_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sql_error)?
            .unwrap_or(0);
        transaction
            .execute(
                "INSERT INTO thoughts(
                    id, session_id, content, annotations_json, position, created_at, updated_at,
                    collapsed, presentation, deleted_at, editor_history_cursor
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    content = excluded.content,
                    annotations_json = excluded.annotations_json,
                    position = excluded.position,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    collapsed = excluded.collapsed,
                    presentation = excluded.presentation,
                    deleted_at = excluded.deleted_at",
                params![
                    thought.id.database_bytes().as_slice(),
                    thought.session_id.database_bytes().as_slice(),
                    thought.content,
                    annotations_json,
                    i64::from(thought.position.get()),
                    thought.created_at.as_millis(),
                    thought.updated_at.as_millis(),
                    i64::from(thought.presentation.is_collapsed()),
                    thought.presentation.as_str(),
                    thought.deleted_at.map(Timestamp::as_millis),
                    cursor,
                ],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}

fn persist_separators(
    transaction: &Transaction<'_>,
    board: &SessionBoard,
) -> Result<(), StoreError> {
    for separator in board.separators() {
        transaction
            .execute(
                "INSERT INTO separators(
                    id, session_id, position, created_at, updated_at, deleted_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    position = excluded.position,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                params![
                    separator.id.database_bytes().as_slice(),
                    separator.session_id.database_bytes().as_slice(),
                    i64::from(separator.position.get()),
                    separator.created_at.as_millis(),
                    separator.updated_at.as_millis(),
                    separator.deleted_at.map(Timestamp::as_millis),
                ],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}
