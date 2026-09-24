//! Atomic session metadata changes.

use std::path::Path;

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{OperationId, SessionId, Timestamp},
    ports::store::{
        BrowserCommitReceipt, NamedSessionCreation, NamedSessionMatch, NamedSessionOutcome,
        NamedSessionPolicy, SessionRequest, StoreError,
    },
};

use super::{
    browser_history::{ReceiptReplay, RequestReceipt},
    search::rebuild_session_search,
    support::{map_sql_error, path_from_bytes, path_to_bytes, session_id_from_blob},
};

pub(super) fn record_open(
    transaction: &Transaction<'_>,
    id: SessionId,
    cwd: &Path,
    at: Timestamp,
) -> Result<(), StoreError> {
    if !cwd.is_absolute() {
        return Err(StoreError::Invariant(
            "session directory must be absolute".to_owned(),
        ));
    }
    super::browser_history::invalidate_activity_conflicts(transaction, id)?;
    let changed = transaction
        .execute(
            "UPDATE sessions SET last_opened_cwd = ?2, last_opened_at = ?3,
             last_active_at = max(last_active_at, ?3) WHERE id = ?1 AND deleted_at IS NULL",
            params![
                id.database_bytes().as_slice(),
                path_to_bytes(cwd),
                at.as_millis()
            ],
        )
        .map_err(map_sql_error)?;
    require_changed(changed, id)?;
    rebuild_session_search(transaction, id)
}

pub(super) fn rename(
    transaction: &Transaction<'_>,
    id: SessionId,
    name: Option<&str>,
) -> Result<(), StoreError> {
    if name.is_some_and(|value| value.trim().is_empty()) {
        return Err(StoreError::Invariant(
            "session name cannot be blank".to_owned(),
        ));
    }
    let changed = transaction
        .execute(
            "UPDATE sessions SET name = ?2 WHERE id = ?1",
            params![id.database_bytes().as_slice(), name],
        )
        .map_err(map_sql_error)?;
    require_changed(changed, id)?;
    rebuild_session_search(transaction, id)
}

fn require_changed(changed: usize, id: SessionId) -> Result<(), StoreError> {
    if changed == 0 {
        Err(StoreError::NotFound(id.to_string()))
    } else {
        Ok(())
    }
}

pub(super) fn trash(
    transaction: &Transaction<'_>,
    id: SessionId,
    at: Timestamp,
) -> Result<(), StoreError> {
    let changed = transaction
        .execute(
            "UPDATE sessions SET deleted_at = ?2, last_active_at = max(last_active_at, ?2) WHERE id = ?1",
            params![id.database_bytes().as_slice(), at.as_millis()],
        )
        .map_err(map_sql_error)?;
    require_changed(changed, id)?;
    rebuild_session_search(transaction, id)
}

pub(super) fn restore(transaction: &Transaction<'_>, id: SessionId) -> Result<(), StoreError> {
    let changed = transaction
        .execute(
            "UPDATE sessions SET deleted_at = NULL WHERE id = ?1",
            [id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;
    require_changed(changed, id)?;
    rebuild_session_search(transaction, id)
}

pub(super) fn prune(transaction: &Transaction<'_>, id: SessionId) -> Result<(), StoreError> {
    let deleted: Option<Option<i64>> = transaction
        .query_row(
            "SELECT deleted_at FROM sessions WHERE id = ?1",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;
    match deleted {
        None => return Err(StoreError::NotFound(id.to_string())),
        Some(None) => {
            return Err(StoreError::Conflict(
                "live sessions must be trashed before pruning".to_owned(),
            ));
        }
        Some(Some(_)) => {}
    }
    transaction
        .execute(
            "DELETE FROM session_search WHERE session_id = ?1",
            [id.to_string()],
        )
        .map_err(map_sql_error)?;
    super::browser_history::remove_session(transaction, id)?;
    transaction
        .execute(
            "DELETE FROM sessions WHERE id = ?1",
            [id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

/// Prune one session and retain the request identity after its history is removed.
pub(super) fn prune_request(
    transaction: &Transaction<'_>,
    id: SessionId,
    operation_id: OperationId,
    at: Timestamp,
) -> Result<BrowserCommitReceipt, StoreError> {
    let request = SessionRequest::Prune { session_id: id };
    match super::browser_history::replay_request(transaction, operation_id, &request)? {
        ReceiptReplay::Matched(receipt) => return Ok(receipt),
        ReceiptReplay::Reused => {
            return Err(StoreError::Conflict(
                "operation identity was reused for another session request".to_owned(),
            ));
        }
        ReceiptReplay::Absent => {}
    }
    prune(transaction, id)?;
    super::browser_history::insert_request_receipt(
        transaction,
        &RequestReceipt::Prune {
            operation_id,
            session_id: id,
        },
        at,
    )
}

/// Insert one named session unless its identity or policy says otherwise.
pub(super) fn create_named(
    transaction: &Transaction<'_>,
    creation: &NamedSessionCreation,
) -> Result<NamedSessionOutcome, StoreError> {
    let session = &creation.session;
    let Some(name) = session.name.clone() else {
        return Err(StoreError::Invariant(
            "named session creation requires a name".to_owned(),
        ));
    };
    session
        .validate()
        .map_err(|error| StoreError::Invariant(error.to_string()))?;
    let receipt = creation
        .operation_id
        .map(|operation_id| RequestReceipt::Create {
            operation_id,
            session_id: session.id,
            name: name.clone(),
            origin_cwd: session.origin_cwd.clone(),
        });
    if let Some(RequestReceipt::Create {
        operation_id,
        session_id,
        name,
        origin_cwd,
    }) = &receipt
    {
        let request = SessionRequest::Create {
            session_id: *session_id,
            name: name.clone(),
            origin_cwd: origin_cwd.clone(),
        };
        match super::browser_history::replay_request(transaction, *operation_id, &request)? {
            ReceiptReplay::Matched(_) => return Ok(NamedSessionOutcome::Replayed),
            ReceiptReplay::Reused => return Ok(NamedSessionOutcome::IdentityReused),
            ReceiptReplay::Absent => {}
        }
    }
    if receipt.is_some() && session_exists(transaction, session.id)? {
        return Ok(NamedSessionOutcome::IdentityReused);
    }
    if creation.policy == NamedSessionPolicy::UnlessNameExists {
        let existing = live_sessions_named(transaction, &name)?;
        if !existing.is_empty() {
            return Ok(NamedSessionOutcome::NameInUse(existing));
        }
    }
    super::board_commit::create_session(transaction, session)?;
    if let Some(receipt) = &receipt {
        super::browser_history::insert_request_receipt(transaction, receipt, session.created_at)?;
    }
    Ok(NamedSessionOutcome::Created)
}

fn session_exists(transaction: &Transaction<'_>, id: SessionId) -> Result<bool, StoreError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)
}

fn live_sessions_named(
    transaction: &Transaction<'_>,
    name: &str,
) -> Result<Vec<NamedSessionMatch>, StoreError> {
    let mut statement = transaction
        .prepare(
            "SELECT id, origin_cwd FROM sessions
             WHERE name = ?1 AND deleted_at IS NULL
             ORDER BY created_at, id",
        )
        .map_err(map_sql_error)?;
    let rows = statement
        .query_map([name], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(map_sql_error)?;
    let mut matches = Vec::new();
    for row in rows {
        let (id, origin_cwd) = row.map_err(map_sql_error)?;
        matches.push(NamedSessionMatch {
            id: session_id_from_blob(id)?,
            origin_cwd: path_from_bytes(origin_cwd)?,
        });
    }
    Ok(matches)
}
