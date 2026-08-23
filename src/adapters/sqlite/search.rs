//! Derived full-text search and session result projection.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    domain::SessionId,
    ports::store::{SessionHit, SessionQuery, StoreError},
};

use super::{
    load::{load_integration_context, load_session_record},
    support::{i64_to_usize, map_sql_error, session_id_from_blob},
};

pub(super) fn rebuild_session_search(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "DELETE FROM session_search WHERE session_id = ?1",
            [session_id.to_string()],
        )
        .map_err(map_sql_error)?;
    let session = load_session_record(transaction, session_id)?;
    let content: String = transaction
        .query_row(
            "SELECT coalesce(group_concat(content, char(10)), '') FROM (
                SELECT content FROM thoughts
                WHERE session_id = ?1 AND deleted_at IS NULL ORDER BY position
             )",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let paths = format!(
        "{}\n{}",
        session.origin_cwd.to_string_lossy(),
        session.last_opened_cwd.to_string_lossy()
    );
    transaction
        .execute(
            "INSERT INTO session_search(session_id, name, paths, content) VALUES (?1, ?2, ?3, ?4)",
            params![session_id.to_string(), session.name, paths, content],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

pub(super) fn search_ids(
    connection: &Connection,
    query: &SessionQuery,
) -> Result<Vec<SessionId>, StoreError> {
    let text = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let mut output = Vec::new();
    if let Some(text) = text {
        let phrase = text
            .split_whitespace()
            .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");
        let mut statement = connection
            .prepare("SELECT session_id FROM session_search WHERE session_search MATCH ?1")
            .map_err(map_sql_error)?;
        let rows = statement
            .query_map([phrase], |row| row.get::<_, String>(0))
            .map_err(map_sql_error)?;
        for row in rows {
            let id: SessionId = row
                .map_err(map_sql_error)?
                .parse()
                .map_err(|error| StoreError::Corrupt(format!("invalid FTS session ID: {error}")))?;
            let trashed = session_is_trashed(connection, id)?;
            if query.include_trashed || !trashed {
                output.push(id);
            }
        }
    } else {
        let sql = if query.include_trashed {
            "SELECT id FROM sessions"
        } else {
            "SELECT id FROM sessions WHERE deleted_at IS NULL"
        };
        let mut statement = connection.prepare(sql).map_err(map_sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sql_error)?;
        for row in rows {
            output.push(session_id_from_blob(row.map_err(map_sql_error)?)?);
        }
    }
    Ok(output)
}

pub(super) fn load_hit(
    connection: &Connection,
    session_id: SessionId,
) -> Result<SessionHit, StoreError> {
    let session = load_session_record(connection, session_id)?;
    let count: i64 = connection
        .query_row(
            "SELECT count(*) FROM thoughts WHERE session_id = ?1 AND deleted_at IS NULL",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let mut statement = connection
        .prepare(
            "SELECT content FROM thoughts
             WHERE session_id = ?1 AND deleted_at IS NULL ORDER BY position",
        )
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map([session_id.database_bytes().as_slice()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(map_sql_error)?;
    let mut previews = Vec::new();
    for row in rows {
        let content = row.map_err(map_sql_error)?;
        if !content.trim().is_empty() {
            previews.push(content.graphemes(true).take(160).collect());
            if previews.len() == 2 {
                break;
            }
        }
    }
    let excerpt = previews.first().cloned().unwrap_or_default();
    Ok(SessionHit {
        id: session.id,
        name: session.name,
        origin_cwd: session.origin_cwd,
        last_opened_cwd: session.last_opened_cwd,
        last_opened_at: session.last_opened_at,
        last_active_at: session.last_active_at,
        thought_count: i64_to_usize(count)?,
        excerpt,
        previews,
        integration_context: load_integration_context(connection, session_id)?,
        trashed: session.deleted_at.is_some(),
    })
}

fn session_is_trashed(connection: &Connection, id: SessionId) -> Result<bool, StoreError> {
    let deleted: Option<i64> = connection
        .query_row(
            "SELECT deleted_at FROM sessions WHERE id = ?1",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?
        .ok_or_else(|| StoreError::Corrupt("FTS references missing session".to_owned()))?;
    Ok(deleted.is_some())
}
