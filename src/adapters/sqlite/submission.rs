//! Durable, content-redacted submission attempt journal.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{BoardOperation, SessionId, SubmissionId, Timestamp},
    ports::{
        agent::AgentState,
        store::{
            CommitReceipt, OperationBatch, StoreError, StoredOperationRequest, SubmissionAttempt,
            SubmissionAttemptState, SubmissionOutcome,
        },
    },
};

use super::support::map_sql_error;

pub(super) fn prepare(
    transaction: &Transaction<'_>,
    attempt: &SubmissionAttempt,
) -> Result<(), StoreError> {
    let first_source = attempt.sources.first().ok_or_else(|| {
        StoreError::Integrity("submission must contain at least one source thought".to_owned())
    })?;
    transaction
        .execute(
            "INSERT INTO submission_attempts(
                id, session_id, thought_id, source_digest, source_sequence,
                disposition, direction, provider, protocol, target_fingerprint,
                pre_state, state, prepared_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'prepared', ?12, ?12)",
            params![
                attempt.id.database_bytes().as_slice(),
                attempt.session_id.database_bytes().as_slice(),
                first_source.thought_id.database_bytes().as_slice(),
                attempt.payload_digest.as_slice(),
                i64::try_from(attempt.source_sequence.get()).unwrap_or(i64::MAX),
                attempt.disposition.as_str(),
                attempt.direction.as_str(),
                attempt.provider,
                i64::from(attempt.protocol),
                attempt.target_fingerprint.as_slice(),
                attempt.pre_state.as_str(),
                attempt.prepared_at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    for (ordinal, source) in attempt.sources.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO submission_attempt_items(
                    submission_id, thought_id, ordinal, source_digest, active
                 ) VALUES (?1, ?2, ?3, ?4, 1)",
                params![
                    attempt.id.database_bytes().as_slice(),
                    source.thought_id.database_bytes().as_slice(),
                    i64::try_from(ordinal).unwrap_or(i64::MAX),
                    source.source_digest.as_slice(),
                ],
            )
            .map_err(map_sql_error)?;
    }
    Ok(())
}

pub(super) fn mark_sending(
    transaction: &Transaction<'_>,
    id: SubmissionId,
    at: Timestamp,
) -> Result<(), StoreError> {
    transition(transaction, id, "prepared", "sending", at, None)
}

pub(super) fn finish(
    transaction: &Transaction<'_>,
    id: SubmissionId,
    outcome: &SubmissionOutcome,
) -> Result<(), StoreError> {
    if matches!(
        outcome.state,
        SubmissionAttemptState::Prepared | SubmissionAttemptState::Sending
    ) {
        return Err(StoreError::Integrity(
            "submission outcome must be terminal".to_owned(),
        ));
    }
    let changed = transaction
        .execute(
            "UPDATE submission_attempts
             SET state = ?2, post_state = ?3, error_code = ?4,
                 deletion_operation_id = ?5, updated_at = ?6
             WHERE id = ?1 AND state = 'sending'",
            params![
                id.database_bytes().as_slice(),
                outcome.state.as_str(),
                outcome.post_state.map(AgentState::as_str),
                outcome.error_code,
                outcome
                    .deletion_operation_id
                    .map(|operation| operation.database_bytes().to_vec()),
                outcome.at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    require_one(changed, id)?;
    deactivate_sources(transaction, id)
}

pub(super) fn finish_with_removal(
    transaction: &Transaction<'_>,
    id: SubmissionId,
    outcome: &SubmissionOutcome,
    removal: &BoardOperation,
) -> Result<CommitReceipt, StoreError> {
    if outcome.state != SubmissionAttemptState::Accepted
        || outcome.deletion_operation_id != Some(removal.id)
    {
        return Err(StoreError::Integrity(
            "submission removal must match an accepted outcome".to_owned(),
        ));
    }
    match finish(transaction, id, outcome) {
        Ok(()) => {
            super::board_commit::commit_batch(transaction, &OperationBatch::Board(removal.clone()))?
                .ok_or_else(|| {
                    StoreError::Integrity("submission removal has no durable receipt".to_owned())
                })
        }
        Err(conflict @ StoreError::Conflict(_)) => {
            if !outcome_matches(transaction, id, outcome)? {
                return Err(conflict);
            }
            match super::operation_lookup::operation_request(transaction, removal.id)? {
                Some(StoredOperationRequest::Board { operation, receipt })
                    if operation.as_ref() == removal =>
                {
                    Ok(receipt)
                }
                _ => Err(conflict),
            }
        }
        Err(error) => Err(error),
    }
}

fn outcome_matches(
    transaction: &Transaction<'_>,
    id: SubmissionId,
    outcome: &SubmissionOutcome,
) -> Result<bool, StoreError> {
    type StoredOutcome = (String, Option<String>, Option<String>, Option<Vec<u8>>, i64);
    let stored: Option<StoredOutcome> = transaction
        .query_row(
            "SELECT state, post_state, error_code, deletion_operation_id, updated_at
             FROM submission_attempts WHERE id = ?1",
            [id.database_bytes().as_slice()],
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
    let Some((state, post_state, error_code, deletion_operation_id, updated_at)) = stored else {
        return Ok(false);
    };
    Ok(state == outcome.state.as_str()
        && post_state.as_deref() == outcome.post_state.map(AgentState::as_str)
        && error_code == outcome.error_code
        && deletion_operation_id
            == outcome
                .deletion_operation_id
                .map(|operation| operation.database_bytes().to_vec())
        && updated_at == outcome.at.as_millis())
}

pub(super) fn recover(
    transaction: &Transaction<'_>,
    session_id: SessionId,
    at: Timestamp,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "UPDATE submission_attempts
             SET state = CASE state
                 WHEN 'prepared' THEN 'cancelled'
                 WHEN 'sending' THEN 'outcome_unknown'
                 ELSE state END,
                 updated_at = ?2
             WHERE session_id = ?1 AND state IN ('prepared', 'sending')",
            params![session_id.database_bytes().as_slice(), at.as_millis()],
        )
        .map_err(map_sql_error)?;
    transaction
        .execute(
            "UPDATE submission_attempt_items SET active = 0
             WHERE submission_id IN (
                 SELECT id FROM submission_attempts WHERE session_id = ?1
             )",
            params![session_id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

fn deactivate_sources(transaction: &Transaction<'_>, id: SubmissionId) -> Result<(), StoreError> {
    transaction
        .execute(
            "UPDATE submission_attempt_items SET active = 0 WHERE submission_id = ?1",
            [id.database_bytes().as_slice()],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

fn transition(
    transaction: &Transaction<'_>,
    id: SubmissionId,
    from: &str,
    to: &str,
    at: Timestamp,
    error_code: Option<&str>,
) -> Result<(), StoreError> {
    let changed = transaction
        .execute(
            "UPDATE submission_attempts SET state = ?2, error_code = ?3, updated_at = ?4
             WHERE id = ?1 AND state = ?5",
            params![
                id.database_bytes().as_slice(),
                to,
                error_code,
                at.as_millis(),
                from
            ],
        )
        .map_err(map_sql_error)?;
    require_one(changed, id)
}

fn require_one(changed: usize, id: SubmissionId) -> Result<(), StoreError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(StoreError::Conflict(format!(
            "submission {id} is not in the expected state"
        )))
    }
}
