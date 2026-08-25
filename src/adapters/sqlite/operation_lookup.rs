//! Typed lookup for durable operation idempotency records.

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    domain::{BoardOperation, OperationId, OperationSequence, SessionId, Timestamp, UndoScope},
    ports::store::{CommitReceipt, DurableIdentity, StoreError, StoredOperationRequest},
};

use super::support::{map_sql_error, sequence_from_i64, session_id_from_blob};

pub(super) fn operation_request(
    connection: &Connection,
    id: OperationId,
) -> Result<Option<StoredOperationRequest>, StoreError> {
    let row: Option<(Vec<u8>, i64, String)> = connection
        .query_row(
            "SELECT session_id, sequence, request_json FROM commit_receipts
             WHERE entity_kind = 'operation' AND external_id = ?1",
            params![id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some((session, sequence, request)) = row else {
        return Ok(None);
    };
    let session_id = session_id_from_blob(session)?;
    let sequence = sequence_from_i64(sequence)?;
    let receipt = CommitReceipt {
        session_id,
        sequence,
        identity: DurableIdentity::Operation(id),
        idempotent_replay: true,
    };
    if let Some((replay, _)) = super::receipt_compaction::decode(&request)? {
        return Ok(Some(StoredOperationRequest::Compacted { replay, receipt }));
    }
    if let Ok(operation) = serde_json::from_str::<BoardOperation>(&request) {
        validate_board_record(&operation, id, session_id, sequence)?;
        return Ok(Some(StoredOperationRequest::Board {
            operation: Box::new(operation),
            receipt,
        }));
    }
    let (stored_session, scope, undo, stored_sequence, _at): (
        SessionId,
        UndoScope,
        bool,
        OperationSequence,
        Timestamp,
    ) = serde_json::from_str(&request).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    if stored_session != session_id || stored_sequence != sequence {
        return Err(StoreError::Corrupt(
            "operation receipt does not match its history request".to_owned(),
        ));
    }
    Ok(Some(StoredOperationRequest::HistoryMove {
        session_id,
        scope,
        undo,
        receipt,
    }))
}

fn validate_board_record(
    operation: &BoardOperation,
    id: OperationId,
    session_id: SessionId,
    sequence: OperationSequence,
) -> Result<(), StoreError> {
    if operation.id != id || operation.session_id != session_id || operation.sequence != sequence {
        return Err(StoreError::Corrupt(
            "operation receipt does not match its board request".to_owned(),
        ));
    }
    Ok(())
}
