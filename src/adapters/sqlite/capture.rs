//! Atomic screenshot receipt and thought creation.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    domain::{BoardMutation, Timestamp},
    ports::{
        screenshot::ScreenshotFingerprint,
        store::{CaptureCommit, CaptureCommitOutcome, CaptureReceipt, StoreError},
    },
};

use super::{
    board_commit::commit_board,
    support::{map_sql_error, operation_id_from_blob, session_id_from_blob, thought_id_from_blob},
};

pub(super) fn commit(
    transaction: &Transaction<'_>,
    capture: &CaptureCommit,
) -> Result<CaptureCommitOutcome, StoreError> {
    if let Some(existing) = existing(transaction, capture.source)? {
        return Ok(CaptureCommitOutcome::AlreadyCaptured(existing));
    }
    let durable = commit_board(transaction, &capture.operation)?;
    let BoardMutation::AddThought { thought } = &capture.operation.forward else {
        return Err(StoreError::Invariant(
            "screenshot capture must add exactly one thought".to_owned(),
        ));
    };
    let thought_id = thought.id;
    let receipt = CaptureReceipt {
        source: capture.source,
        session_id: capture.operation.session_id,
        thought_id,
        operation_id: capture.operation.id,
        accepted_at: capture.operation.created_at,
    };
    transaction
        .execute(
            "INSERT INTO screenshot_capture_receipts(
                source_fingerprint, session_id, thought_id, operation_id, accepted_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                capture.source.0.as_slice(),
                receipt.session_id.database_bytes().as_slice(),
                receipt.thought_id.database_bytes().as_slice(),
                receipt.operation_id.database_bytes().as_slice(),
                receipt.accepted_at.as_millis(),
            ],
        )
        .map_err(map_sql_error)?;
    Ok(CaptureCommitOutcome::Created {
        durable,
        capture: receipt,
    })
}

fn existing(
    transaction: &Transaction<'_>,
    source: ScreenshotFingerprint,
) -> Result<Option<CaptureReceipt>, StoreError> {
    let row = transaction
        .query_row(
            "SELECT session_id, thought_id, operation_id, accepted_at
             FROM screenshot_capture_receipts WHERE source_fingerprint = ?1",
            [source.0.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(map_sql_error)?;
    row.map(|(session, thought, operation, accepted_at)| {
        Ok(CaptureReceipt {
            source,
            session_id: session_id_from_blob(session)?,
            thought_id: thought_id_from_blob(thought)?,
            operation_id: operation_id_from_blob(operation)?,
            accepted_at: Timestamp::from_millis(accepted_at),
        })
    })
    .transpose()
}
