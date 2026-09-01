//! Atomic version claim, session creation, and ordinary practice-thought insertion.

use rusqlite::{Transaction, params};

use crate::ports::store::{FirstRunBoard, FirstRunOutcome, StoreError};

use super::{
    board_commit::create_session, history_commit::persist_board, search::rebuild_session_search,
    support::map_sql_error,
};

pub(super) fn create(
    transaction: &Transaction<'_>,
    candidate: &FirstRunBoard,
) -> Result<FirstRunOutcome, StoreError> {
    let completed: i64 = transaction
        .query_row(
            "SELECT completed_version FROM onboarding_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    let completed = u32::try_from(completed)
        .map_err(|_| StoreError::Corrupt("invalid onboarding version".to_owned()))?;
    let board = candidate.board();
    create_session(transaction, &board.session)?;
    if completed >= candidate.version().get() {
        return Ok(FirstRunOutcome::AlreadyCompleted);
    }
    persist_board(transaction, board)?;
    rebuild_session_search(transaction, board.session.id)?;
    let changed = transaction
        .execute(
            "UPDATE onboarding_state SET completed_version = ?1
             WHERE singleton = 1 AND completed_version < ?1",
            params![i64::from(candidate.version().get())],
        )
        .map_err(map_sql_error)?;
    if changed != 1 {
        return Err(StoreError::Conflict(
            "onboarding eligibility changed during its transaction".to_owned(),
        ));
    }
    Ok(FirstRunOutcome::Seeded)
}
