//! Content-redacted envelopes for durable operation receipts.

use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    domain::{BoardMutation, BoardOperation, BoardOperationKind},
    ports::store::{CompactedOperationRequest, StoreError, thought_payload_digest},
};

use super::support::map_sql_error;

const ENVELOPE_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
struct ReceiptEnvelope {
    compacted_version: u32,
    request_sha256: [u8; 32],
    replay: CompactedOperationRequest,
}

pub(super) fn board_replay(
    operation: &BoardOperation,
) -> Result<CompactedOperationRequest, StoreError> {
    let replay = match (&operation.kind, &operation.forward) {
        (BoardOperationKind::Create, BoardMutation::AddThought { thought }) => {
            CompactedOperationRequest::Add {
                session_id: operation.session_id,
                thought_id: thought.id,
                payload_digest: thought_payload_digest(&thought.content, &thought.annotations)?,
                position: usize::try_from(thought.position.get()).map_err(|_| {
                    StoreError::Corrupt("thought position cannot fit in memory".to_owned())
                })?,
            }
        }
        (
            BoardOperationKind::Delete
            | BoardOperationKind::Cut
            | BoardOperationKind::SubmitAndRemove,
            BoardMutation::SetDeletion {
                thought_id,
                deleted_at: Some(_),
                ..
            },
        ) => CompactedOperationRequest::Delete {
            session_id: operation.session_id,
            thought_id: *thought_id,
        },
        (BoardOperationKind::Reorder, BoardMutation::MoveThought { thought_id, to, .. }) => {
            CompactedOperationRequest::Move {
                session_id: operation.session_id,
                thought_id: *thought_id,
                position: usize::try_from(to.get()).map_err(|_| {
                    StoreError::Corrupt("thought position cannot fit in memory".to_owned())
                })?,
            }
        }
        _ => CompactedOperationRequest::Opaque,
    };
    Ok(replay)
}

pub(super) fn encode(
    original: &str,
    replay: CompactedOperationRequest,
) -> Result<String, StoreError> {
    serde_json::to_string(&ReceiptEnvelope {
        compacted_version: ENVELOPE_VERSION,
        request_sha256: request_digest(original),
        replay,
    })
    .map_err(|error| StoreError::Serialization(error.to_string()))
}

pub(super) fn decode(
    value: &str,
) -> Result<Option<(CompactedOperationRequest, [u8; 32])>, StoreError> {
    let Ok(envelope) = serde_json::from_str::<ReceiptEnvelope>(value) else {
        return Ok(None);
    };
    if envelope.compacted_version != ENVELOPE_VERSION {
        return Err(StoreError::Corrupt(
            "unsupported compacted receipt version".to_owned(),
        ));
    }
    Ok(Some((envelope.replay, envelope.request_sha256)))
}

pub(super) fn matches_original(compacted: &str, expected: &str) -> Result<bool, StoreError> {
    Ok(decode(compacted)?.is_some_and(|(_, digest)| digest == request_digest(expected)))
}

pub(super) fn compact_receipt(
    transaction: &Transaction<'_>,
    entity_kind: &str,
    external_id: [u8; 16],
    replay: CompactedOperationRequest,
) -> Result<(), StoreError> {
    let request: String = transaction
        .query_row(
            "SELECT request_json FROM commit_receipts
             WHERE entity_kind = ?1 AND external_id = ?2",
            params![entity_kind, external_id.as_slice()],
            |row| row.get(0),
        )
        .map_err(map_sql_error)?;
    if decode(&request)?.is_some() {
        return Ok(());
    }
    let compacted = encode(&request, replay)?;
    transaction
        .execute(
            "UPDATE commit_receipts SET request_json = ?3
             WHERE entity_kind = ?1 AND external_id = ?2",
            params![entity_kind, external_id.as_slice(), compacted],
        )
        .map_err(map_sql_error)?;
    Ok(())
}

fn request_digest(request: &str) -> [u8; 32] {
    Sha256::digest(request.as_bytes()).into()
}
