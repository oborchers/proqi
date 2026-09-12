//! Durable Browser request receipts that intentionally do not create history.

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{BrowserMutation, BrowserOperationKind, OperationId, SessionId, Timestamp},
    ports::store::{BrowserCommitReceipt, StoreError},
};

use super::{codec::decode, cursor_and_count, ensure_commit_id_unused};
use crate::adapters::sqlite::support::{i64_to_usize, map_sql_error, usize_to_i64};

const RECEIPT_KIND: &str = "rename_noop_v1";

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct NoOpRenameReceipt {
    receipt: String,
    operation_id: OperationId,
    session_id: SessionId,
    name: Option<String>,
}

pub(in crate::adapters::sqlite) fn commit_noop_rename(
    transaction: &Transaction<'_>,
    operation_id: OperationId,
    session_id: SessionId,
    name: Option<&str>,
    at: Timestamp,
) -> Result<BrowserCommitReceipt, StoreError> {
    validate_name(name)?;
    let receipt = NoOpRenameReceipt {
        receipt: RECEIPT_KIND.to_owned(),
        operation_id,
        session_id,
        name: name.map(str::to_owned),
    };
    let payload = serde_json::to_string(&receipt)
        .map_err(|_| StoreError::Serialization("Browser receipt encoding failed".to_owned()))?;
    if let Some(existing) = existing_receipt(transaction, &receipt)? {
        return Ok(existing);
    }
    ensure_commit_id_unused(transaction, operation_id)?;
    require_current_name(transaction, session_id, name)?;
    let cursor = cursor_and_count(transaction)?.0;
    transaction
        .execute(
            "INSERT INTO browser_operation_receipts(
                id, target_session_id, payload_json, cursor, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                operation_id.database_bytes().as_slice(),
                session_id.database_bytes().as_slice(),
                payload,
                usize_to_i64(cursor)?,
                at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    Ok(BrowserCommitReceipt {
        operation_id,
        cursor,
        idempotent_replay: false,
    })
}

fn existing_receipt(
    transaction: &Transaction<'_>,
    requested: &NoOpRenameReceipt,
) -> Result<Option<BrowserCommitReceipt>, StoreError> {
    let existing: Option<(Vec<u8>, String, i64)> = transaction
        .query_row(
            "SELECT target_session_id, payload_json, cursor
             FROM browser_operation_receipts WHERE id = ?1",
            [requested.operation_id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some((target, payload, cursor)) = existing else {
        return Ok(None);
    };
    if !receipt_matches(&payload, &target, requested)? {
        return Err(StoreError::Conflict(
            "Browser operation identity was reused for different content".to_owned(),
        ));
    }
    Ok(Some(BrowserCommitReceipt {
        operation_id: requested.operation_id,
        cursor: i64_to_usize(cursor)?,
        idempotent_replay: true,
    }))
}

fn receipt_matches(
    payload: &str,
    target: &[u8],
    requested: &NoOpRenameReceipt,
) -> Result<bool, StoreError> {
    if let Some(stored) = decode_noop(payload)? {
        validate_metadata(
            stored.operation_id,
            stored.session_id,
            requested.operation_id,
            target,
        )?;
        return Ok(stored == *requested);
    }
    let operation = decode(payload)?;
    validate_metadata(
        operation.id(),
        operation.session_id(),
        requested.operation_id,
        target,
    )?;
    Ok(operation.session_id() == requested.session_id
        && operation.kind() == BrowserOperationKind::Rename
        && matches!(
            operation.forward(),
            BrowserMutation::SetName { value, .. } if value == &requested.name
        ))
}

fn validate_metadata(
    stored_id: OperationId,
    stored_session: SessionId,
    expected_id: OperationId,
    target: &[u8],
) -> Result<(), StoreError> {
    if stored_id != expected_id || stored_session.database_bytes().as_slice() != target {
        Err(StoreError::Corrupt(
            "Browser operation receipt metadata does not match its payload".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn decode_noop(payload: &str) -> Result<Option<NoOpRenameReceipt>, StoreError> {
    let value: serde_json::Value = serde_json::from_str(payload)
        .map_err(|_| StoreError::Corrupt("invalid Browser operation receipt".to_owned()))?;
    if value.get("receipt").and_then(serde_json::Value::as_str) != Some(RECEIPT_KIND) {
        return Ok(None);
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(|_| StoreError::Corrupt("invalid Browser no-op receipt".to_owned()))
}

pub(super) fn is_noop_receipt(
    payload: &str,
    operation_id: OperationId,
    target: &[u8],
) -> Result<bool, StoreError> {
    let Some(stored) = decode_noop(payload)? else {
        return Ok(false);
    };
    validate_metadata(stored.operation_id, stored.session_id, operation_id, target)?;
    Ok(true)
}

fn require_current_name(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    expected: Option<&str>,
) -> Result<(), StoreError> {
    let current: Option<Option<String>> = transaction
        .query_row(
            "SELECT name FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    let Some(current) = current else {
        return Err(StoreError::NotFound(session_id.to_string()));
    };
    if current.as_deref() != expected {
        return Err(StoreError::Conflict(
            "session name changed before Browser receipt commit".to_owned(),
        ));
    }
    Ok(())
}

fn validate_name(name: Option<&str>) -> Result<(), StoreError> {
    if name.is_some_and(|value| value.trim().is_empty()) {
        Err(StoreError::Invariant(
            "session name cannot be blank".to_owned(),
        ))
    } else {
        Ok(())
    }
}
