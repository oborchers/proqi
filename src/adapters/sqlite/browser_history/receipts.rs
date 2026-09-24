//! Durable session-administration request receipts and their replay lookup.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{OperationId, SessionId, Timestamp},
    ports::store::{
        BrowserCommitReceipt, SessionRequest, SessionRequestReceipt, StoreError,
        StoredSessionRequest,
    },
};

use super::{
    cursor_and_count,
    request_codec::{RequestReceipt, RetainedPayload, decode_retained},
};
use crate::adapters::sqlite::support::{
    i64_to_usize, map_sql_error, operation_id_from_blob, usize_to_i64,
};

/// Replay outcome for one request identity inside the current transaction.
pub(in crate::adapters::sqlite) enum ReceiptReplay {
    /// The identity is unused.
    Absent,
    /// The identity already committed exactly this request.
    Matched(BrowserCommitReceipt),
    /// The identity belongs to another request.
    Reused,
}

pub(in crate::adapters::sqlite) fn commit_noop_rename(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
    session_id: SessionId,
    name: Option<&str>,
    at: Timestamp,
) -> Result<BrowserCommitReceipt, StoreError> {
    if name.is_some_and(|value| value.trim().is_empty()) {
        return Err(StoreError::Invariant(
            "session name cannot be blank".to_owned(),
        ));
    }
    let request = SessionRequest::Rename {
        session_id,
        name: name.map(str::to_owned),
    };
    if let Some(receipt) = replayed(transaction, operation_id, &request)? {
        return Ok(receipt);
    }
    let current = current_session(transaction, session_id)?;
    if current.name.as_deref() != name {
        return Err(StoreError::Conflict(
            "session name changed before Browser receipt commit".to_owned(),
        ));
    }
    insert(
        transaction,
        &RequestReceipt::RenameNoOp {
            operation_id,
            session_id,
            name: name.map(str::to_owned),
        },
        at,
    )
}

pub(in crate::adapters::sqlite) fn commit_noop_trash(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
    session_id: SessionId,
    at: Timestamp,
) -> Result<BrowserCommitReceipt, StoreError> {
    let request = SessionRequest::Trash { session_id };
    if let Some(receipt) = replayed(transaction, operation_id, &request)? {
        return Ok(receipt);
    }
    if !current_session(transaction, session_id)?.trashed {
        return Err(StoreError::Conflict(
            "session left trash before the trash receipt commit".to_owned(),
        ));
    }
    insert(
        transaction,
        &RequestReceipt::TrashNoOp {
            operation_id,
            session_id,
        },
        at,
    )
}

/// Compare one identity with the request that owns it.
pub(in crate::adapters::sqlite) fn replay(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
    request: &SessionRequest,
) -> Result<ReceiptReplay, StoreError> {
    Ok(match stored_request(transaction, operation_id)? {
        None => ReceiptReplay::Absent,
        Some(StoredSessionRequest::Administration(stored)) if stored.request == *request => {
            ReceiptReplay::Matched(BrowserCommitReceipt {
                operation_id,
                cursor: stored.cursor,
                idempotent_replay: true,
            })
        }
        Some(_) => ReceiptReplay::Reused,
    })
}

/// Insert one receipt that does not move Browser history.
pub(in crate::adapters::sqlite) fn insert(
    transaction: &Transaction<'_>,
    receipt: &RequestReceipt,
    at: Timestamp,
) -> Result<BrowserCommitReceipt, StoreError> {
    let payload = receipt.encode()?;
    let cursor = cursor_and_count(transaction)?.0;
    transaction
        .execute(
            "INSERT INTO browser_operation_receipts(
                id, target_session_id, payload_json, cursor, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                receipt.operation_id().database_bytes().as_slice(),
                receipt.session_id().database_bytes().as_slice(),
                payload,
                usize_to_i64(cursor)?,
                at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    Ok(BrowserCommitReceipt {
        operation_id: receipt.operation_id(),
        cursor,
        idempotent_replay: false,
    })
}

/// Look up every durable owner of one operation identity.
pub(in crate::adapters::sqlite) fn stored_request(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
) -> Result<Option<StoredSessionRequest>, StoreError> {
    let id = operation_id.database_bytes();
    let retained: Option<(Vec<u8>, String, i64)> = transaction
        .query_row(
            "SELECT target_session_id, payload_json, cursor
             FROM browser_operation_receipts WHERE id = ?1",
            [id.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    if let Some((target, payload, cursor)) = retained {
        let request = decode_retained(&payload, operation_id, &target)?.request()?;
        return Ok(Some(StoredSessionRequest::Administration(
            SessionRequestReceipt {
                operation_id,
                request,
                cursor: i64_to_usize(cursor)?,
                history_target: None,
            },
        )));
    }
    if let Some(receipt) = history_request(transaction, operation_id)? {
        return Ok(Some(StoredSessionRequest::Administration(receipt)));
    }
    let used_by_session_history: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM commit_receipts WHERE external_id = ?1)",
            [id.as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    Ok(used_by_session_history.then_some(StoredSessionRequest::SessionHistory))
}

/// Whether a retained Browser payload is a request receipt rather than history.
pub(in crate::adapters::sqlite) fn is_request_receipt(
    payload: &str,
    operation_id: OperationId,
    target: &[u8],
) -> Result<bool, StoreError> {
    Ok(matches!(
        decode_retained(payload, operation_id, target)?,
        RetainedPayload::Receipt(_)
    ))
}

fn replayed(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
    request: &SessionRequest,
) -> Result<Option<BrowserCommitReceipt>, StoreError> {
    match replay(transaction, operation_id, request)? {
        ReceiptReplay::Absent => Ok(None),
        ReceiptReplay::Matched(receipt) => Ok(Some(receipt)),
        ReceiptReplay::Reused => Err(StoreError::Conflict(
            "Browser operation identity was reused for different content".to_owned(),
        )),
    }
}

fn history_request(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
) -> Result<Option<SessionRequestReceipt>, StoreError> {
    let stored: Option<(Vec<u8>, bool, i64)> = transaction
        .query_row(
            "SELECT target_operation_id, undo, cursor
             FROM browser_history_receipts WHERE id = ?1",
            [operation_id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some((target, undo, cursor)) = stored else {
        return Ok(None);
    };
    let target = operation_id_from_blob(target)?;
    let target_payload: Option<(Vec<u8>, String)> = transaction
        .query_row(
            "SELECT target_session_id, payload_json
             FROM browser_operation_receipts WHERE id = ?1",
            [target.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let history_target = target_payload
        .map(|(session, payload)| decode_retained(&payload, target, &session))
        .transpose()?
        .and_then(|retained| match retained {
            RetainedPayload::Operation(operation) => Some(operation.kind()),
            RetainedPayload::Receipt(_) => None,
        });
    Ok(Some(SessionRequestReceipt {
        operation_id,
        request: SessionRequest::History { undo },
        cursor: i64_to_usize(cursor)?,
        history_target,
    }))
}

struct CurrentSession {
    name: Option<String>,
    trashed: bool,
}

fn current_session(
    transaction: &Transaction<'_>,
    session_id: SessionId,
) -> Result<CurrentSession, StoreError> {
    transaction
        .query_row(
            "SELECT name, deleted_at IS NOT NULL FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| {
                Ok(CurrentSession {
                    name: row.get(0)?,
                    trashed: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(map_sql_error)?
        .ok_or_else(|| StoreError::NotFound(session_id.to_string()))
}
