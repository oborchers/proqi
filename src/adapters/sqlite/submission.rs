//! Durable, content-redacted submission attempt journal.

use rusqlite::{Transaction, params};

use crate::{
    domain::{Direction, SessionId, SubmissionId, Timestamp},
    ports::{
        agent::{AgentState, SubmissionDisposition},
        store::{StoreError, SubmissionAttempt, SubmissionAttemptState, SubmissionOutcome},
    },
};

use super::support::map_sql_error;

pub(super) fn prepare(
    transaction: &Transaction<'_>,
    attempt: &SubmissionAttempt,
) -> Result<(), StoreError> {
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
                attempt.thought_id.database_bytes().as_slice(),
                attempt.source_digest.as_slice(),
                i64::try_from(attempt.source_sequence.get()).unwrap_or(i64::MAX),
                disposition_name(attempt.disposition),
                direction_name(attempt.direction),
                attempt.provider,
                i64::from(attempt.protocol),
                attempt.target_fingerprint.as_slice(),
                state_name(attempt.pre_state),
                attempt.prepared_at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
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
                attempt_state_name(outcome.state),
                outcome.post_state.map(state_name),
                outcome.error_code,
                outcome
                    .deletion_operation_id
                    .map(|operation| operation.database_bytes().to_vec()),
                outcome.at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    require_one(changed, id)
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

const fn disposition_name(value: SubmissionDisposition) -> &'static str {
    match value {
        SubmissionDisposition::Keep => "keep",
        SubmissionDisposition::RemoveAfterSuccess => "remove_after_success",
    }
}

const fn direction_name(value: Direction) -> &'static str {
    match value {
        Direction::Up => "up",
        Direction::Right => "right",
        Direction::Down => "down",
        Direction::Left => "left",
    }
}

const fn state_name(value: AgentState) -> &'static str {
    match value {
        AgentState::Idle => "idle",
        AgentState::Working => "working",
        AgentState::Done => "done",
        AgentState::Blocked => "blocked",
        AgentState::Unknown => "unknown",
    }
}

const fn attempt_state_name(value: SubmissionAttemptState) -> &'static str {
    match value {
        SubmissionAttemptState::Prepared => "prepared",
        SubmissionAttemptState::Sending => "sending",
        SubmissionAttemptState::Accepted => "accepted",
        SubmissionAttemptState::Failed => "failed",
        SubmissionAttemptState::Cancelled => "cancelled",
        SubmissionAttemptState::OutcomeUnknown => "outcome_unknown",
    }
}
