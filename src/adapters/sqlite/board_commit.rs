//! New structural and editor commits.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{
        BoardMutation, BoardOperation, ContentAnnotation, OperationSequence, Session, SessionId,
        ThoughtId, ThoughtRevision, Timestamp,
    },
    ports::store::{CommitReceipt, DurableIdentity, OperationBatch, StoreError},
};

use super::{
    history_commit::{
        HistoryMove, commit_history_move, existing_receipt, insert_receipt, persist_board,
        require_next_sequence, set_integration_context, update_session_sequence,
    },
    load::{load_board, load_session_record},
    search::rebuild_session_search,
    support::{map_sql_error, path_to_bytes, sequence_to_i64, session_id_from_blob, usize_to_i64},
};

pub(super) fn commit_batch(
    transaction: &Transaction<'_>,
    batch: &OperationBatch,
) -> Result<Option<CommitReceipt>, StoreError> {
    match batch {
        OperationBatch::CreateSession(session) => {
            create_session(transaction, session)?;
            Ok(None)
        }
        OperationBatch::Board(operation) => commit_board(transaction, operation).map(Some),
        OperationBatch::Revision(revision) => commit_revision(transaction, revision).map(Some),
        OperationBatch::HistoryMove {
            operation_id,
            session_id,
            scope,
            undo,
            sequence,
            at,
        } => commit_history_move(
            transaction,
            HistoryMove {
                operation_id: *operation_id,
                session_id: *session_id,
                scope: *scope,
                undo: *undo,
                sequence: *sequence,
                at: *at,
            },
        )
        .map(Some),
        OperationBatch::IntegrationContext {
            session_id,
            context,
        } => {
            set_integration_context(transaction, *session_id, context.as_ref())?;
            Ok(None)
        }
    }
}

pub(super) fn create_session(
    transaction: &Transaction<'_>,
    session: &Session,
) -> Result<(), StoreError> {
    if session.last_durable_sequence != OperationSequence::ZERO {
        return Err(StoreError::Conflict(
            "new sessions must start at operation sequence zero".to_owned(),
        ));
    }
    let existing: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT id FROM sessions WHERE id = ?1",
            [session.id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    if existing.is_some() {
        let loaded = load_session_record(transaction, session.id)?;
        if &loaded == session {
            return Ok(());
        }
        return Err(StoreError::Conflict(format!(
            "session identifier already exists: {}",
            session.id
        )));
    }
    transaction
        .execute(
            "INSERT INTO sessions (
                id, name, origin_cwd, last_opened_cwd, created_at, last_opened_at,
                last_active_at, last_durable_sequence, board_history_cursor, deleted_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)",
            params![
                session.id.database_bytes().as_slice(),
                session.name,
                path_to_bytes(&session.origin_cwd),
                path_to_bytes(&session.last_opened_cwd),
                session.created_at.as_millis(),
                session.last_opened_at.as_millis(),
                session.last_active_at.as_millis(),
                sequence_to_i64(session.last_durable_sequence)?,
                session.deleted_at.map(Timestamp::as_millis),
            ],
        )
        .map_err(map_sql_error)?;
    rebuild_session_search(transaction, session.id)
}

pub(super) fn commit_board(
    transaction: &Transaction<'_>,
    operation: &BoardOperation,
) -> Result<CommitReceipt, StoreError> {
    let request_json = serde_json::to_string(operation)
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    if let Some(receipt) = existing_receipt(
        transaction,
        "operation",
        operation.id.database_bytes(),
        &request_json,
        DurableIdentity::Operation(operation.id),
    )? {
        return Ok(receipt);
    }
    require_next_sequence(transaction, operation.session_id, operation.sequence)?;
    let mut board = load_board(transaction, operation.session_id)?;
    board
        .apply_mutation(&operation.forward, operation.created_at)
        .map_err(|error| StoreError::Invariant(error.to_string()))?;
    let cursor: i64 = transaction
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [operation.session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let cursor = super::support::i64_to_usize(cursor)?;
    transaction
        .execute(
            "DELETE FROM board_operations WHERE session_id = ?1 AND history_index >= ?2",
            params![
                operation.session_id.database_bytes().as_slice(),
                usize_to_i64(cursor)?
            ],
        )
        .map_err(map_sql_error)?;
    truncate_editor_redo(transaction, &operation.forward)?;
    persist_board(transaction, &board)?;
    transaction
        .execute(
            "INSERT INTO board_operations(id, session_id, history_index, sequence, payload_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                operation.id.database_bytes().as_slice(),
                operation.session_id.database_bytes().as_slice(),
                usize_to_i64(cursor)?,
                sequence_to_i64(operation.sequence)?,
                request_json,
                operation.created_at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    insert_receipt(
        transaction,
        operation.session_id,
        operation.sequence,
        "operation",
        operation.id.database_bytes(),
        &request_json,
        operation.created_at,
    )?;
    transaction
        .execute(
            "UPDATE sessions SET board_history_cursor = ?2, last_durable_sequence = ?3,
             last_active_at = max(last_active_at, ?4) WHERE id = ?1",
            params![
                operation.session_id.database_bytes().as_slice(),
                usize_to_i64(cursor + 1)?,
                sequence_to_i64(operation.sequence)?,
                operation.created_at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    if mutation_changes_search(&operation.forward) {
        rebuild_session_search(transaction, operation.session_id)?;
    }
    Ok(CommitReceipt {
        session_id: operation.session_id,
        sequence: operation.sequence,
        identity: DurableIdentity::Operation(operation.id),
        idempotent_replay: false,
    })
}

pub(super) fn mutation_changes_search(mutation: &BoardMutation) -> bool {
    match mutation {
        BoardMutation::Batch { mutations } => mutations.iter().any(mutation_changes_search),
        BoardMutation::AddThought { .. }
        | BoardMutation::SetDeletion { .. }
        | BoardMutation::SetDeletionExact { .. }
        | BoardMutation::ReplaceContent { .. } => true,
        BoardMutation::MoveThought { .. }
        | BoardMutation::SetPresentation { .. }
        | BoardMutation::LegacySetCollapsed { .. } => false,
    }
}

fn truncate_editor_redo(
    transaction: &Transaction<'_>,
    mutation: &BoardMutation,
) -> Result<(), StoreError> {
    match mutation {
        BoardMutation::Batch { mutations } => {
            for mutation in mutations {
                truncate_editor_redo(transaction, mutation)?;
            }
        }
        BoardMutation::ReplaceContent { thought_id, .. }
        | BoardMutation::SetDeletionExact { thought_id, .. } => {
            transaction
                .execute(
                    "DELETE FROM thought_revisions WHERE thought_id = ?1 AND history_index >=
                     (SELECT editor_history_cursor FROM thoughts WHERE id = ?1)",
                    [thought_id.database_bytes().as_slice()],
                )
                .map_err(map_sql_error)?;
        }
        BoardMutation::AddThought { .. }
        | BoardMutation::SetDeletion { .. }
        | BoardMutation::MoveThought { .. }
        | BoardMutation::SetPresentation { .. }
        | BoardMutation::LegacySetCollapsed { .. } => {}
    }
    Ok(())
}

fn commit_revision(
    transaction: &Transaction<'_>,
    revision: &ThoughtRevision,
) -> Result<CommitReceipt, StoreError> {
    let request_json = serde_json::to_string(revision)
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    if let Some(receipt) = existing_receipt(
        transaction,
        "revision",
        revision.id.database_bytes(),
        &request_json,
        DurableIdentity::Revision(revision.id),
    )? {
        return Ok(receipt);
    }
    require_next_sequence(transaction, revision.session_id, revision.sequence)?;
    let cursor = revision_cursor(transaction, revision)?;
    truncate_conflicting_board_redo(transaction, revision.session_id, revision.thought_id)?;
    persist_revision(transaction, revision, cursor, &request_json)?;
    insert_receipt(
        transaction,
        revision.session_id,
        revision.sequence,
        "revision",
        revision.id.database_bytes(),
        &request_json,
        revision.created_at,
    )?;
    update_session_sequence(
        transaction,
        revision.session_id,
        revision.sequence,
        revision.created_at,
    )?;
    rebuild_session_search(transaction, revision.session_id)?;
    Ok(CommitReceipt {
        session_id: revision.session_id,
        sequence: revision.sequence,
        identity: DurableIdentity::Revision(revision.id),
        idempotent_replay: false,
    })
}

fn truncate_conflicting_board_redo(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    thought_id: ThoughtId,
) -> Result<(), StoreError> {
    let cursor: i64 = transaction
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let mut statement = transaction
        .prepare(
            "SELECT history_index, payload_json FROM board_operations
             WHERE session_id = ?1 AND history_index >= ?2 ORDER BY history_index",
        )
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map(
            params![session_id.database_bytes().as_slice(), cursor],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(map_sql_error)?;
    let mut first_conflict = None;
    for row in rows {
        let (history_index, payload) = row.map_err(map_sql_error)?;
        let operation: BoardOperation = serde_json::from_str(&payload)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
        if operation.forward.addresses(thought_id) || operation.inverse.addresses(thought_id) {
            first_conflict = Some(history_index);
            break;
        }
    }
    drop(statement);
    if let Some(history_index) = first_conflict {
        transaction
            .execute(
                "DELETE FROM board_operations WHERE session_id = ?1 AND history_index >= ?2",
                params![session_id.database_bytes().as_slice(), history_index],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}

fn revision_cursor(
    transaction: &Transaction<'_>,
    revision: &ThoughtRevision,
) -> Result<i64, StoreError> {
    type CurrentRevisionRow = (Vec<u8>, String, String, i64, Option<i64>);
    let current: Option<CurrentRevisionRow> = transaction
        .query_row(
            "SELECT session_id, content, annotations_json, editor_history_cursor, deleted_at
             FROM thoughts WHERE id = ?1",
            [revision.thought_id.database_bytes().as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(map_sql_error)?;
    let (stored_session, content, annotations_json, cursor, deleted_at) =
        current.ok_or_else(|| StoreError::NotFound(revision.thought_id.to_string()))?;
    let annotations: Vec<ContentAnnotation> = serde_json::from_str(&annotations_json)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    if session_id_from_blob(stored_session)? != revision.session_id
        || deleted_at.is_some()
        || content != revision.before_content
        || annotations != revision.before_annotations
    {
        return Err(StoreError::Conflict(format!(
            "revision precondition failed: {}",
            revision.id
        )));
    }
    Ok(cursor)
}

fn persist_revision(
    transaction: &Transaction<'_>,
    revision: &ThoughtRevision,
    cursor: i64,
    request_json: &str,
) -> Result<(), StoreError> {
    let annotations_json = serde_json::to_string(&revision.after_annotations)
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    transaction
        .execute(
            "DELETE FROM thought_revisions WHERE thought_id = ?1 AND history_index >= ?2",
            params![revision.thought_id.database_bytes().as_slice(), cursor],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "INSERT INTO thought_revisions(
                id, session_id, thought_id, history_index, sequence, payload_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                revision.id.database_bytes().as_slice(),
                revision.session_id.database_bytes().as_slice(),
                revision.thought_id.database_bytes().as_slice(),
                cursor,
                sequence_to_i64(revision.sequence)?,
                request_json,
                revision.created_at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE thoughts SET content = ?2, annotations_json = ?3, updated_at = ?4,
                    editor_history_cursor = ?5 WHERE id = ?1",
            params![
                revision.thought_id.database_bytes().as_slice(),
                revision.after_content,
                annotations_json,
                revision.created_at.as_millis(),
                cursor
                    .checked_add(1)
                    .ok_or_else(|| StoreError::Corrupt("editor cursor overflow".to_owned()))?,
            ],
        )
        .map_err(map_sql_error)?;
    Ok(())
}
