//! Retry-safe durable receipts for same-value thought-name requests.

use rusqlite::Transaction;

use crate::{
    domain::{OperationId, OperationSequence, SessionId, ThoughtId, ThoughtName, Timestamp},
    ports::store::{CommitReceipt, CompactedOperationRequest, DurableIdentity, StoreError},
};

use super::super::{
    browser_history,
    history_commit::{insert_receipt, require_next_sequence, update_session_sequence},
    load::load_board,
    operation_lookup, receipt_compaction,
};

pub(super) fn commit_noop_rename(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
    session_id: SessionId,
    thought_id: ThoughtId,
    name: Option<&ThoughtName>,
    sequence: OperationSequence,
    at: Timestamp,
) -> Result<CommitReceipt, StoreError> {
    let replay = CompactedOperationRequest::Rename {
        session_id,
        thought_id,
        name: name.cloned(),
    };
    if let Some(existing) = operation_lookup::operation_request(transaction, operation_id)? {
        return match existing {
            crate::ports::store::StoredOperationRequest::Compacted {
                replay: stored,
                receipt,
            } if stored == replay => Ok(receipt),
            _ => Err(StoreError::Conflict(
                "operation identity was reused for different content".to_owned(),
            )),
        };
    }
    browser_history::ensure_not_used_by_browser_history(
        transaction,
        operation_id.database_bytes(),
    )?;
    require_next_sequence(transaction, session_id, sequence)?;
    let board = load_board(transaction, session_id)?;
    let thought = board
        .thought(thought_id)
        .filter(|thought| thought.is_live())
        .ok_or_else(|| StoreError::NotFound(thought_id.to_string()))?;
    if thought.name.as_ref() != name {
        return Err(StoreError::Conflict(
            "thought name changed before no-op receipt".to_owned(),
        ));
    }
    let original = serde_json::to_string(&(session_id, thought_id, name, sequence, at))
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    let request = receipt_compaction::encode(&original, replay)?;
    insert_receipt(
        transaction,
        session_id,
        sequence,
        "operation",
        operation_id.database_bytes(),
        &request,
        at,
    )?;
    update_session_sequence(transaction, session_id, sequence, at)?;
    Ok(CommitReceipt {
        session_id,
        sequence,
        identity: DurableIdentity::Operation(operation_id),
        idempotent_replay: false,
    })
}
