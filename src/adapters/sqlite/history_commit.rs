//! Persistent board and editor history movements.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{
        BoardOperation, ContentAnnotation, IntegrationContext, OperationId, OperationSequence,
        SessionBoard, SessionId, ThoughtId, ThoughtRevision, Timestamp, UndoScope,
    },
    ports::store::{CommitReceipt, DurableIdentity, StoreError},
};

use super::{
    board_commit::mutation_changes_search,
    load::load_board,
    search::rebuild_session_search,
    support::{
        map_sql_error, sequence_from_i64, sequence_to_i64, session_id_from_blob, usize_to_i64,
    },
};

#[derive(Clone, Copy)]
pub(super) struct HistoryMove {
    pub(super) operation_id: OperationId,
    pub(super) session_id: SessionId,
    pub(super) scope: UndoScope,
    pub(super) undo: bool,
    pub(super) sequence: OperationSequence,
    pub(super) at: Timestamp,
}

pub(super) fn commit_history_move(
    transaction: &Transaction<'_>,
    request: HistoryMove,
) -> Result<CommitReceipt, StoreError> {
    let HistoryMove {
        operation_id,
        session_id,
        scope,
        undo,
        sequence,
        at,
    } = request;
    let request_json = serde_json::to_string(&(session_id, scope, undo, sequence, at))
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    if let Some(receipt) = existing_receipt(
        transaction,
        "operation",
        operation_id.database_bytes(),
        &request_json,
        DurableIdentity::Operation(operation_id),
    )? {
        return Ok(receipt);
    }
    require_next_sequence(transaction, session_id, sequence)?;
    let search_changed = match scope {
        UndoScope::Board => move_board_history(transaction, session_id, undo, at)?,
        UndoScope::Editor { thought_id } => {
            move_editor_history(transaction, session_id, thought_id, undo, at)?;
            true
        }
    };
    insert_receipt(
        transaction,
        session_id,
        sequence,
        "operation",
        operation_id.database_bytes(),
        &request_json,
        at,
    )?;
    update_session_sequence(transaction, session_id, sequence, at)?;
    if search_changed {
        rebuild_session_search(transaction, session_id)?;
    }
    Ok(CommitReceipt {
        session_id,
        sequence,
        identity: DurableIdentity::Operation(operation_id),
        idempotent_replay: false,
    })
}

fn move_board_history(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    undo: bool,
    at: Timestamp,
) -> Result<bool, StoreError> {
    let cursor: i64 = transaction
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?
        .ok_or_else(|| StoreError::NotFound(session_id.to_string()))?;
    let index = if undo {
        cursor
            .checked_sub(1)
            .ok_or_else(|| StoreError::Conflict("board undo history is empty".to_owned()))?
    } else {
        cursor
    };
    let payload: Option<String> = transaction
        .query_row(
            "SELECT payload_json FROM board_operations WHERE session_id = ?1 AND history_index = ?2",
            params![session_id.database_bytes().as_slice(), index],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    let operation: BoardOperation = serde_json::from_str(
        &payload.ok_or_else(|| StoreError::Conflict("board redo history is empty".to_owned()))?,
    )
    .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let mutation = if undo {
        &operation.inverse
    } else {
        &operation.forward
    };
    let mut board = load_board(transaction, session_id)?;
    board
        .apply_mutation(mutation, at)
        .map_err(|error| StoreError::Invariant(error.to_string()))?;
    persist_board(transaction, &board)?;
    let new_cursor = if undo {
        index
    } else {
        cursor
            .checked_add(1)
            .ok_or_else(|| StoreError::Corrupt("board cursor overflow".to_owned()))?
    };
    transaction
        .execute(
            "UPDATE sessions SET board_history_cursor = ?2 WHERE id = ?1",
            params![session_id.database_bytes().as_slice(), new_cursor],
        )
        .map_err(map_sql_error)?;
    Ok(mutation_changes_search(mutation))
}

fn move_editor_history(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    thought_id: ThoughtId,
    undo: bool,
    at: Timestamp,
) -> Result<(), StoreError> {
    let current = load_editor_current(transaction, thought_id)?;
    if session_id_from_blob(current.session_id)? != session_id {
        return Err(StoreError::Conflict(
            "editor history belongs to another session".to_owned(),
        ));
    }
    let index = if undo {
        current
            .cursor
            .checked_sub(1)
            .ok_or_else(|| StoreError::Conflict("editor undo history is empty".to_owned()))?
    } else {
        current.cursor
    };
    let revision = load_revision_at(transaction, thought_id, index)?;
    let expected_content = if undo {
        &revision.after_content
    } else {
        &revision.before_content
    };
    let expected_annotations = if undo {
        &revision.after_annotations
    } else {
        &revision.before_annotations
    };
    if &current.content != expected_content || &current.annotations != expected_annotations {
        return Err(StoreError::Conflict(
            "editor history does not match current thought content".to_owned(),
        ));
    }
    let content = if undo {
        revision.before_content
    } else {
        revision.after_content
    };
    let annotations = if undo {
        revision.before_annotations
    } else {
        revision.after_annotations
    };
    let annotations_json = serde_json::to_string(&annotations)
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    let new_cursor = if undo {
        index
    } else {
        current
            .cursor
            .checked_add(1)
            .ok_or_else(|| StoreError::Corrupt("editor cursor overflow".to_owned()))?
    };
    transaction
        .execute(
            "UPDATE thoughts SET content = ?2, annotations_json = ?3, updated_at = ?4,
                    editor_history_cursor = ?5 WHERE id = ?1",
            params![
                thought_id.database_bytes().as_slice(),
                content,
                annotations_json,
                at.as_millis(),
                new_cursor
            ],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

struct EditorCurrent {
    session_id: Vec<u8>,
    content: String,
    annotations: Vec<ContentAnnotation>,
    cursor: i64,
}

fn load_editor_current(
    transaction: &Transaction<'_>,
    thought_id: ThoughtId,
) -> Result<EditorCurrent, StoreError> {
    type Row = (Vec<u8>, String, String, i64);
    let row: Option<Row> = transaction
        .query_row(
            "SELECT session_id, content, annotations_json, editor_history_cursor FROM thoughts
             WHERE id = ?1 AND deleted_at IS NULL",
            [thought_id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let (session_id, content, annotations_json, cursor) =
        row.ok_or_else(|| StoreError::NotFound(thought_id.to_string()))?;
    let annotations = serde_json::from_str(&annotations_json)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    Ok(EditorCurrent {
        session_id,
        content,
        annotations,
        cursor,
    })
}

fn load_revision_at(
    transaction: &Transaction<'_>,
    thought_id: ThoughtId,
    index: i64,
) -> Result<ThoughtRevision, StoreError> {
    let payload: Option<String> = transaction
        .query_row(
            "SELECT payload_json FROM thought_revisions WHERE thought_id = ?1 AND history_index = ?2",
            params![thought_id.database_bytes().as_slice(), index],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    serde_json::from_str(
        &payload.ok_or_else(|| StoreError::Conflict("editor redo history is empty".to_owned()))?,
    )
    .map_err(|error| StoreError::Corrupt(error.to_string()))
}

pub(super) fn existing_receipt(
    transaction: &Transaction<'_>,
    entity_kind: &str,
    external_id: [u8; 16],
    expected_request: &str,
    identity: DurableIdentity,
) -> Result<Option<CommitReceipt>, StoreError> {
    let existing: Option<(Vec<u8>, i64, String)> = transaction
        .query_row(
            "SELECT session_id, sequence, request_json FROM commit_receipts
             WHERE entity_kind = ?1 AND external_id = ?2",
            params![entity_kind, external_id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some((session, sequence, request)) = existing else {
        return Ok(None);
    };
    if request != expected_request
        && !super::receipt_compaction::matches_original(&request, expected_request)?
    {
        return Err(StoreError::Conflict(
            "idempotency identity was reused for another request".to_owned(),
        ));
    }
    Ok(Some(CommitReceipt {
        session_id: session_id_from_blob(session)?,
        sequence: sequence_from_i64(sequence)?,
        identity,
        idempotent_replay: true,
    }))
}

pub(super) fn insert_receipt(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    sequence: OperationSequence,
    entity_kind: &str,
    external_id: [u8; 16],
    request_json: &str,
    at: Timestamp,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "INSERT INTO commit_receipts(
                session_id, sequence, entity_kind, external_id, request_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id.database_bytes().as_slice(),
                sequence_to_i64(sequence)?,
                entity_kind,
                external_id.as_slice(),
                request_json,
                at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

pub(super) fn require_next_sequence(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    sequence: OperationSequence,
) -> Result<(), StoreError> {
    let current: Option<i64> = transaction
        .query_row(
            "SELECT last_durable_sequence FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    let current = current.ok_or_else(|| StoreError::NotFound(session_id.to_string()))?;
    let expected = current
        .checked_add(1)
        .ok_or_else(|| StoreError::Corrupt("operation sequence overflow".to_owned()))?;
    if sequence_to_i64(sequence)? != expected {
        return Err(StoreError::Conflict(format!(
            "expected operation sequence {expected}, received {}",
            sequence.get()
        )));
    }
    Ok(())
}

pub(super) fn update_session_sequence(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    sequence: OperationSequence,
    at: Timestamp,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "UPDATE sessions SET last_durable_sequence = ?2,
             last_active_at = max(last_active_at, ?3) WHERE id = ?1",
            params![
                session_id.database_bytes().as_slice(),
                sequence_to_i64(sequence)?,
                at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

pub(super) fn persist_board(
    transaction: &Transaction<'_>,
    board: &SessionBoard,
) -> Result<(), StoreError> {
    let maximum: Option<i64> = transaction
        .query_row(
            "SELECT max(position) FROM thoughts WHERE session_id = ?1 AND deleted_at IS NULL",
            [board.session.id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let offset = maximum
        .unwrap_or(0)
        .checked_add(usize_to_i64(board.live_thoughts().len())?)
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| StoreError::Corrupt("thought position offset overflow".to_owned()))?;
    transaction
        .execute(
            "UPDATE thoughts SET position = position + ?2
             WHERE session_id = ?1 AND deleted_at IS NULL",
            params![board.session.id.database_bytes().as_slice(), offset],
        )
        .map_err(map_sql_error)?;
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

pub(super) fn set_integration_context(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    context: Option<&IntegrationContext>,
) -> Result<(), StoreError> {
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if !exists {
        return Err(StoreError::NotFound(session_id.to_string()));
    }
    if let Some(context) = context {
        let payload = serde_json::to_string(context)
            .map_err(|error| StoreError::Serialization(error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO integration_context(session_id, payload_json) VALUES (?1, ?2)
                 ON CONFLICT(session_id) DO UPDATE SET payload_json = excluded.payload_json",
                params![session_id.database_bytes().as_slice(), payload],
            )
            .map_err(map_sql_error)?;
    } else {
        transaction
            .execute(
                "DELETE FROM integration_context WHERE session_id = ?1",
                [session_id.database_bytes().as_slice()],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}
