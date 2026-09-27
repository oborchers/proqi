//! Retry-safe durable receipts for same-value thought-name requests.

use rusqlite::Transaction;

use crate::{
    domain::{OperationId, OperationSequence, SessionId, ThoughtId, ThoughtName, Timestamp},
    ports::store::{
        CommitReceipt, CompactedOperationRequest, DurableIdentity, SemanticRequestFingerprint,
        StoreError,
    },
};

use super::super::{
    browser_history,
    history_commit::{
        ReceiptInsert, insert_receipt, require_next_sequence, update_session_sequence,
    },
    load::load_board,
    operation_lookup, receipt_compaction,
};

pub(super) struct NoOpRename {
    pub(super) operation_id: OperationId,
    pub(super) session_id: SessionId,
    pub(super) thought_id: ThoughtId,
    pub(super) name: Option<ThoughtName>,
    pub(super) sequence: OperationSequence,
    pub(super) at: Timestamp,
    pub(super) semantic_fingerprint: Option<SemanticRequestFingerprint>,
}

pub(super) fn commit_noop_rename(
    transaction: &Transaction<'_>,
    request: NoOpRename,
) -> Result<CommitReceipt, StoreError> {
    let NoOpRename {
        operation_id,
        session_id,
        thought_id,
        name,
        sequence,
        at,
        semantic_fingerprint,
    } = request;
    let replay = CompactedOperationRequest::Rename {
        session_id,
        thought_id,
        name: name.clone(),
    };
    if let Some(existing) = operation_lookup::operation_request(transaction, operation_id)? {
        return match existing {
            crate::ports::store::StoredOperationRequest::Compacted {
                replay: stored,
                semantic_fingerprint: stored_fingerprint,
                receipt,
            } if stored == replay && stored_fingerprint == semantic_fingerprint => Ok(receipt),
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
    if thought.name != name {
        return Err(StoreError::Conflict(
            "thought name changed before no-op receipt".to_owned(),
        ));
    }
    let original = serde_json::to_string(&(session_id, thought_id, &name, sequence, at))
        .map_err(|error| StoreError::Serialization(error.to_string()))?;
    let request = receipt_compaction::encode(&original, replay)?;
    insert_receipt(
        transaction,
        ReceiptInsert {
            session_id,
            sequence,
            entity_kind: "operation",
            external_id: operation_id.database_bytes(),
            request_json: &request,
            semantic_fingerprint,
            at,
        },
    )?;
    update_session_sequence(transaction, session_id, sequence, at)?;
    Ok(CommitReceipt {
        session_id,
        sequence,
        identity: DurableIdentity::Operation(operation_id),
        idempotent_replay: false,
    })
}
