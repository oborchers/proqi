//! Durable selected-transfer intent and exact cohort acknowledgement.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{BoardMutation, BoardOperation, BoardOperationKind, OperationId, SessionId},
    ports::{
        store::{CommitReceipt, DurableIdentity, StoreError},
        transfer::SessionTransferBatchRequest,
    },
};

use super::{
    SqliteStore, board_commit::commit_board, history_commit::existing_receipt,
    support::map_sql_error,
};

pub(crate) struct PendingTransfer {
    pub(crate) request: SessionTransferBatchRequest,
}

impl SqliteStore {
    pub(crate) fn pending_transfers(
        &self,
        source: SessionId,
    ) -> Result<Vec<PendingTransfer>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT request_json FROM transfer_attempts
                 WHERE source_session_id = ?1 AND status != 'completed' ORDER BY created_at, operation_id",
            )
            .map_err(map_sql_error)?;
        let rows = statement
            .query_map([source.database_bytes().as_slice()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(map_sql_error)?;
        rows.map(|row| {
            let request = row.map_err(map_sql_error)?;
            Ok(PendingTransfer {
                request: serde_json::from_str(&request)
                    .map_err(|error| StoreError::Corrupt(error.to_string()))?,
            })
        })
        .collect()
    }

    pub(crate) fn prepare_transfer(
        &mut self,
        request: &SessionTransferBatchRequest,
        at: crate::domain::Timestamp,
    ) -> Result<Option<CommitReceipt>, StoreError> {
        if request.items.is_empty()
            || request.source_session_id == request.destination_session_id
            || request.operation_id == request.removal_operation_id
        {
            return Err(StoreError::Conflict("invalid transfer cohort".to_owned()));
        }
        let encoded = serde_json::to_string(request)
            .map_err(|error| StoreError::Serialization(error.to_string()))?;
        self.with_write_retry(|transaction| {
            let existing: Option<(String, Option<String>)> = transaction
                .query_row(
                    "SELECT request_json, destination_receipt_json FROM transfer_attempts
                     WHERE operation_id = ?1",
                    [request.operation_id.database_bytes().as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(map_sql_error)?;
            if let Some((stored, receipt)) = existing {
                return replay_prepared(&encoded, &stored, receipt);
            }
            transaction
                .execute(
                    "INSERT INTO transfer_attempts(operation_id, source_session_id,
                     destination_session_id, removal_operation_id, request_json, status, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'prepared', ?6)",
                    params![
                        request.operation_id.database_bytes().as_slice(),
                        request.source_session_id.database_bytes().as_slice(),
                        request.destination_session_id.database_bytes().as_slice(),
                        request.removal_operation_id.database_bytes().as_slice(),
                        encoded,
                        at.as_millis(),
                    ],
                )
                .map_err(map_sql_error)?;
            for item in &request.items {
                transaction
                    .execute(
                        "INSERT INTO transfer_source_claims(source_thought_id, operation_id)
                         VALUES (?1, ?2)",
                        params![
                            item.source_thought_id.database_bytes().as_slice(),
                            request.operation_id.database_bytes().as_slice(),
                        ],
                    )
                    .map_err(map_sql_error)?;
            }
            Ok(None)
        })
    }

    pub(crate) fn mark_transfer_sending(&mut self, id: OperationId) -> Result<(), StoreError> {
        self.with_write_retry(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE transfer_attempts SET status = 'sending'
                     WHERE operation_id = ?1 AND status = 'prepared'",
                    [id.database_bytes().as_slice()],
                )
                .map_err(map_sql_error)?;
            if changed == 0 {
                let status: String = transaction
                    .query_row(
                        "SELECT status FROM transfer_attempts WHERE operation_id = ?1",
                        [id.database_bytes().as_slice()],
                        |row| row.get(0),
                    )
                    .map_err(map_sql_error)?;
                validate_sending_status(&status)?;
            }
            Ok(())
        })
    }

    pub(crate) fn accept_transfer(
        &mut self,
        request: &SessionTransferBatchRequest,
        receipt: CommitReceipt,
    ) -> Result<(), StoreError> {
        if receipt.session_id != request.destination_session_id
            || receipt.identity != DurableIdentity::Operation(request.operation_id)
        {
            return Err(StoreError::Conflict(
                "destination receipt does not match cohort".to_owned(),
            ));
        }
        let encoded = serde_json::to_string(&receipt)
            .map_err(|error| StoreError::Serialization(error.to_string()))?;
        self.with_write_retry(|transaction| {
            let stored: (String, Option<String>) = transaction
                .query_row(
                    "SELECT request_json, destination_receipt_json FROM transfer_attempts
                     WHERE operation_id = ?1",
                    [request.operation_id.database_bytes().as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(map_sql_error)?;
            if stored.0
                != serde_json::to_string(request)
                    .map_err(|error| StoreError::Serialization(error.to_string()))?
            {
                return Err(StoreError::Conflict("transfer payload changed".to_owned()));
            }
            if let Some(previous) = stored.1 {
                let previous: CommitReceipt = serde_json::from_str(&previous)
                    .map_err(|error| StoreError::Corrupt(error.to_string()))?;
                validate_receipt_replay(&previous, &receipt)?;
                return Ok(());
            }
            transaction
                .execute(
                    "UPDATE transfer_attempts SET status = 'accepted', destination_receipt_json = ?2
                     WHERE operation_id = ?1 AND status IN ('prepared', 'sending')",
                    params![request.operation_id.database_bytes().as_slice(), encoded],
                )
                .map_err(map_sql_error)?;
            Ok(())
        })
    }

    pub(crate) fn finish_transfer(
        &mut self,
        request: &SessionTransferBatchRequest,
        removal: Option<&BoardOperation>,
        reason: &str,
    ) -> Result<Option<CommitReceipt>, StoreError> {
        if !matches!(
            (request.remove_source, removal.is_some(), reason),
            (true, true, "removed") | (true, false, "source_changed") | (false, false, "kept")
        ) {
            return Err(StoreError::Conflict(
                "transfer completion mode changed".to_owned(),
            ));
        }
        self.with_write_retry(|transaction| {
            let (stored_request, status, completed_reason): (String, String, Option<String>) = transaction
                .query_row(
                    "SELECT request_json, status, completed_reason FROM transfer_attempts WHERE operation_id = ?1",
                    [request.operation_id.database_bytes().as_slice()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(map_sql_error)?;
            if stored_request != serde_json::to_string(request)
                .map_err(|error| StoreError::Serialization(error.to_string()))?
            {
                return Err(StoreError::Conflict("transfer payload changed".to_owned()));
            }
            if status == "completed" {
                return completed_transfer_receipt(transaction, request, removal, completed_reason.as_deref());
            }
            if status != "accepted" {
                return Err(StoreError::Conflict(
                    "destination cohort is not accepted".to_owned(),
                ));
            }
            let receipt = if let Some(operation) = removal {
                validate_removal(request, operation)?;
                Some(commit_board(transaction, operation, None)?)
            } else {
                None
            };
            transaction
                .execute(
                    "UPDATE transfer_attempts SET status = 'completed', completed_reason = ?2
                     WHERE operation_id = ?1",
                    params![request.operation_id.database_bytes().as_slice(), reason],
                )
                .map_err(map_sql_error)?;
            transaction
                .execute(
                    "DELETE FROM transfer_source_claims WHERE operation_id = ?1",
                    [request.operation_id.database_bytes().as_slice()],
                )
                .map_err(map_sql_error)?;
            Ok(receipt)
        })
    }
}

fn replay_prepared(
    encoded: &str,
    stored: &str,
    receipt: Option<String>,
) -> Result<Option<CommitReceipt>, StoreError> {
    if stored != encoded {
        return Err(StoreError::Conflict(
            "transfer identity was reused".to_owned(),
        ));
    }
    receipt
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| StoreError::Corrupt(error.to_string()))
        })
        .transpose()
}

fn validate_sending_status(status: &str) -> Result<(), StoreError> {
    if matches!(status, "sending" | "accepted" | "completed") {
        Ok(())
    } else {
        Err(StoreError::Conflict("transfer cannot start".to_owned()))
    }
}

fn validate_receipt_replay(
    previous: &CommitReceipt,
    receipt: &CommitReceipt,
) -> Result<(), StoreError> {
    if previous.session_id == receipt.session_id
        && previous.sequence == receipt.sequence
        && previous.identity == receipt.identity
    {
        Ok(())
    } else {
        Err(StoreError::Conflict(
            "destination receipt changed".to_owned(),
        ))
    }
}

fn completed_transfer_receipt(
    transaction: &Transaction<'_>,
    request: &SessionTransferBatchRequest,
    removal: Option<&BoardOperation>,
    reason: Option<&str>,
) -> Result<Option<CommitReceipt>, StoreError> {
    match (reason, removal) {
        (Some("removed"), Some(operation)) => {
            validate_removal(request, operation)?;
            let encoded = serde_json::to_string(operation)
                .map_err(|error| StoreError::Serialization(error.to_string()))?;
            existing_receipt(
                transaction,
                "operation",
                operation.id.database_bytes(),
                &encoded,
                None,
                DurableIdentity::Operation(operation.id),
            )?
            .ok_or_else(|| {
                StoreError::Integrity("completed transfer lost its source receipt".to_owned())
            })
            .map(Some)
        }
        (Some("removed"), None) | (Some("kept" | "source_changed"), Some(_)) => Err(
            StoreError::Conflict("transfer completion mode changed".to_owned()),
        ),
        (Some("kept" | "source_changed"), None) => Ok(None),
        _ => Err(StoreError::Corrupt("invalid completed transfer".to_owned())),
    }
}

fn validate_removal(
    request: &SessionTransferBatchRequest,
    operation: &BoardOperation,
) -> Result<(), StoreError> {
    let expected = request
        .items
        .iter()
        .map(|item| item.source_thought_id)
        .collect::<Vec<_>>();
    let mut actual = Vec::new();
    collect_deleted(&operation.forward, &mut actual);
    actual.reverse();
    if operation.id != request.removal_operation_id
        || operation.session_id != request.source_session_id
        || operation.kind != BoardOperationKind::TransferAndRemove
        || actual != expected
    {
        return Err(StoreError::Conflict(
            "source removal does not match transfer".to_owned(),
        ));
    }
    Ok(())
}

fn collect_deleted(mutation: &BoardMutation, output: &mut Vec<crate::domain::ThoughtId>) {
    match mutation {
        BoardMutation::Batch { mutations } => {
            for mutation in mutations {
                collect_deleted(mutation, output);
            }
        }
        BoardMutation::SetDeletion {
            thought_id,
            deleted_at: Some(_),
            ..
        } => output.push(*thought_id),
        _ => {}
    }
}

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod tests;
