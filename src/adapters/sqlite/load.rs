//! Canonical snapshot loading and corruption validation.

use rusqlite::{Connection, OptionalExtension};

use crate::{
    domain::{
        BoardOperation, ContentAnnotation, IntegrationContext, Session, SessionBoard, SessionId,
        Thought, ThoughtId, ThoughtPosition, ThoughtPresentation, ThoughtRevision, Timestamp,
    },
    ports::store::{SessionSnapshot, StoreError},
};

use super::support::{
    i64_to_u32, i64_to_usize, map_sql_error, operation_id_from_blob, path_from_bytes,
    revision_id_from_blob, sequence_from_i64, session_id_from_blob, thought_id_from_blob,
    validate_commit_sequence,
};

pub(super) fn load_snapshot(
    connection: &Connection,
    session_id: SessionId,
) -> Result<SessionSnapshot, StoreError> {
    let session = load_session_record(connection, session_id)?;
    let thoughts = load_thoughts(connection, session_id)?;
    let board = SessionBoard::new(session, thoughts)
        .map_err(|error| StoreError::Invariant(error.to_string()))?;
    let board_history_cursor: i64 = connection
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let board_operations = load_board_operations(connection, session_id)?;
    let revisions = load_revisions(connection, session_id)?;
    let editor_history_cursors = load_editor_cursors(connection, session_id)?;
    let integration_context = load_integration_context(connection, session_id)?;
    let cursor = i64_to_usize(board_history_cursor)?;
    if cursor > board_operations.len() {
        return Err(StoreError::Corrupt(
            "board history cursor exceeds retained operations".to_owned(),
        ));
    }
    for (thought_id, cursor) in &editor_history_cursors {
        let retained = revisions
            .iter()
            .filter(|revision| revision.thought_id == *thought_id)
            .count();
        if *cursor > retained {
            return Err(StoreError::Corrupt(format!(
                "editor history cursor exceeds revisions for {thought_id}"
            )));
        }
    }
    validate_commit_sequence(connection, session_id, board.session.last_durable_sequence)?;
    Ok(SessionSnapshot {
        board,
        board_operations,
        board_history_cursor: cursor,
        revisions,
        editor_history_cursors,
        integration_context,
    })
}

pub(super) fn load_board(
    connection: &Connection,
    session_id: SessionId,
) -> Result<SessionBoard, StoreError> {
    SessionBoard::new(
        load_session_record(connection, session_id)?,
        load_thoughts(connection, session_id)?,
    )
    .map_err(|error| StoreError::Invariant(error.to_string()))
}

pub(super) fn load_session_record(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Session, StoreError> {
    type Row = (
        Vec<u8>,
        Option<String>,
        Vec<u8>,
        Vec<u8>,
        i64,
        i64,
        i64,
        i64,
        Option<i64>,
    );
    let row: Option<Row> = connection
        .query_row(
            "SELECT id, name, origin_cwd, last_opened_cwd, created_at, last_opened_at,
                    last_active_at, last_durable_sequence, deleted_at
             FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .optional()
        .map_err(map_sql_error)?;
    let (id, name, origin, last, created, opened, active, sequence, deleted) =
        row.ok_or_else(|| StoreError::NotFound(session_id.to_string()))?;
    let id = session_id_from_blob(id)?;
    if id != session_id {
        return Err(StoreError::Corrupt(
            "session lookup returned another ID".to_owned(),
        ));
    }
    Ok(Session {
        id,
        name,
        origin_cwd: path_from_bytes(origin)?,
        last_opened_cwd: path_from_bytes(last)?,
        created_at: Timestamp::from_millis(created),
        last_opened_at: Timestamp::from_millis(opened),
        last_active_at: Timestamp::from_millis(active),
        last_durable_sequence: sequence_from_i64(sequence)?,
        deleted_at: deleted.map(Timestamp::from_millis),
    })
}

fn load_thoughts(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Vec<Thought>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT id, session_id, content, annotations_json, position, created_at, updated_at, presentation, deleted_at
             FROM thoughts WHERE session_id = ?1 ORDER BY deleted_at IS NOT NULL, position, id",
        )
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map([session_id.database_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        })
        .map_err(map_sql_error)?;
    let mut thoughts = Vec::new();
    for row in rows {
        let (id, owner, content, annotations, position, created, updated, presentation, deleted) =
            row.map_err(map_sql_error)?;
        let annotations: Vec<ContentAnnotation> = serde_json::from_str(&annotations)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        crate::domain::validate_annotations(&content, &annotations)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        thoughts.push(Thought {
            id: thought_id_from_blob(id)?,
            session_id: session_id_from_blob(owner)?,
            content,
            annotations,
            position: ThoughtPosition::new(i64_to_u32(position)?),
            created_at: Timestamp::from_millis(created),
            updated_at: Timestamp::from_millis(updated),
            presentation: ThoughtPresentation::parse(&presentation)
                .map_err(|error| StoreError::Corrupt(error.to_string()))?,
            deleted_at: deleted.map(Timestamp::from_millis),
        });
    }
    Ok(thoughts)
}

fn load_board_operations(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Vec<BoardOperation>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT id, history_index, sequence, payload_json FROM board_operations
             WHERE session_id = ?1 ORDER BY history_index",
        )
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map([session_id.database_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(map_sql_error)?;
    let mut operations = Vec::new();
    for (expected_index, row) in rows.enumerate() {
        let (id, history_index, sequence, payload) = row.map_err(map_sql_error)?;
        if i64_to_usize(history_index)? != expected_index {
            return Err(StoreError::Corrupt(
                "board operation history has a gap".to_owned(),
            ));
        }
        let operation: BoardOperation = serde_json::from_str(&payload)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        if operation.id != operation_id_from_blob(id)?
            || operation.session_id != session_id
            || operation.sequence != sequence_from_i64(sequence)?
        {
            return Err(StoreError::Corrupt(
                "board operation columns disagree with payload".to_owned(),
            ));
        }
        operation
            .validate_annotations()
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        operations.push(operation);
    }
    Ok(operations)
}

fn load_revisions(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Vec<ThoughtRevision>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT id, thought_id, history_index, sequence, payload_json FROM thought_revisions
             WHERE session_id = ?1 ORDER BY thought_id, history_index",
        )
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map([session_id.database_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(map_sql_error)?;
    let mut revisions = Vec::new();
    let mut next_indexes = std::collections::HashMap::new();
    for row in rows {
        let (id, thought_id, history_index, sequence, payload) = row.map_err(map_sql_error)?;
        let thought_id = thought_id_from_blob(thought_id)?;
        let expected_index = next_indexes.entry(thought_id).or_insert(0_usize);
        if i64_to_usize(history_index)? != *expected_index {
            return Err(StoreError::Corrupt(format!(
                "editor revision history has a gap for {thought_id}"
            )));
        }
        *expected_index += 1;
        let revision: ThoughtRevision = serde_json::from_str(&payload)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        if revision.id != revision_id_from_blob(id)?
            || revision.thought_id != thought_id
            || revision.session_id != session_id
            || revision.sequence != sequence_from_i64(sequence)?
        {
            return Err(StoreError::Corrupt(
                "revision columns disagree with payload".to_owned(),
            ));
        }
        revision
            .validate_annotations()
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        revisions.push(revision);
    }
    Ok(revisions)
}

fn load_editor_cursors(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Vec<(ThoughtId, usize)>, StoreError> {
    let mut statement = connection
        .prepare("SELECT id, editor_history_cursor FROM thoughts WHERE session_id = ?1 ORDER BY id")
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map([session_id.database_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(map_sql_error)?;
    let mut cursors = Vec::new();
    for row in rows {
        let (id, cursor) = row.map_err(map_sql_error)?;
        cursors.push((thought_id_from_blob(id)?, i64_to_usize(cursor)?));
    }
    Ok(cursors)
}

pub(super) fn load_integration_context(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Option<IntegrationContext>, StoreError> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload_json FROM integration_context WHERE session_id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    payload
        .map(|payload| {
            serde_json::from_str(&payload).map_err(|error| StoreError::Corrupt(error.to_string()))
        })
        .transpose()
}
