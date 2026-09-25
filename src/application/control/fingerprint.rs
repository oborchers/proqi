//! Content-redacted identities for exact public mutation requests.

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use crate::{
    domain::{BoardItemId, ContentAnnotation, SessionId, ThoughtId, ThoughtName, UndoScope},
    ports::{
        control::ControlMutation,
        store::{SemanticRequestFingerprint, StoreError, thought_payload_digest_with_name},
    },
};

const FINGERPRINT_VERSION: u32 = 1;
const DOMAIN_SEPARATOR: &[u8] = b"proqi-semantic-request\0";
const PRESERVED_PAYLOAD_DOMAIN_SEPARATOR: &[u8] = b"proqi-preserved-thought-payload\0";

#[derive(Serialize)]
struct PreservedPayload<'a> {
    content: &'a str,
    annotations: &'a [ContentAnnotation],
    name: Option<&'a ThoughtName>,
}

#[derive(Serialize)]
struct FingerprintEnvelope {
    version: u32,
    session_id: SessionId,
    request: FingerprintRequest,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum FingerprintRequest {
    Add {
        thought_id: ThoughtId,
        payload_digest: [u8; 32],
        position: Option<u64>,
        preserve_owned_annotations: bool,
    },
    PreserveAddMany {
        items: Vec<(ThoughtId, ThoughtId, [u8; 32])>,
    },
    RenameThought {
        thought_id: ThoughtId,
        name_digest: Option<[u8; 32]>,
    },
    InsertSeparator {
        separator_id: crate::domain::SeparatorId,
        position: Option<u64>,
    },
    DeleteItems {
        item_ids: Vec<BoardItemId>,
    },
    MoveItem {
        item_id: BoardItemId,
        position: u64,
    },
    DuplicateItems {
        item_ids: Vec<BoardItemId>,
    },
    SplitThought {
        thought_id: ThoughtId,
        new_thought_id: ThoughtId,
        expected_digest: [u8; 32],
        at_byte: u64,
    },
    ExtractThought {
        thought_id: ThoughtId,
        new_thought_id: ThoughtId,
        expected_digest: [u8; 32],
        start_byte: u64,
        end_byte: u64,
    },
    MergeThoughts {
        thought_ids: Vec<ThoughtId>,
        expected_digests: Vec<[u8; 32]>,
        separator_digest: [u8; 32],
    },
    ReflowThought {
        thought_id: ThoughtId,
        expected_digest: [u8; 32],
    },
    ExportThoughts {
        thought_ids: Vec<ThoughtId>,
        disposition: crate::domain::ExportDisposition,
        reference_thought_id: Option<ThoughtId>,
        output_path_digest: [u8; 32],
    },
    Replace {
        thought_id: ThoughtId,
        expected_digest: Option<[u8; 32]>,
        content_digest: [u8; 32],
    },
    SetCollapsed {
        thought_id: ThoughtId,
        collapsed: bool,
    },
    History {
        scope: UndoScope,
        undo: bool,
    },
}

pub(super) fn semantic_fingerprint(
    session_id: SessionId,
    mutation: &ControlMutation,
) -> Result<Option<SemanticRequestFingerprint>, StoreError> {
    let Some(request) = canonical_request(mutation)? else {
        return Ok(None);
    };
    let encoded = serde_json::to_vec(&FingerprintEnvelope {
        version: FINGERPRINT_VERSION,
        session_id,
        request,
    })
    .map_err(|error| StoreError::Serialization(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(DOMAIN_SEPARATOR);
    digest.update(encoded);
    Ok(Some(SemanticRequestFingerprint::from_bytes(
        digest.finalize().into(),
    )))
}

fn canonical_request(mutation: &ControlMutation) -> Result<Option<FingerprintRequest>, StoreError> {
    let request = match mutation {
        ControlMutation::Add { .. } | ControlMutation::PreserveAdd { .. } => {
            canonical_add(mutation)?
        }
        ControlMutation::PreserveAddMany { items, .. } => FingerprintRequest::PreserveAddMany {
            items: items
                .iter()
                .map(|item| {
                    Ok((
                        item.source_thought_id,
                        item.destination_thought_id,
                        thought_payload_digest_with_name(
                            &item.content,
                            &item.annotations,
                            item.name.as_ref(),
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, StoreError>>()?,
        },
        ControlMutation::RenameThought {
            thought_id, name, ..
        } => FingerprintRequest::RenameThought {
            thought_id: *thought_id,
            name_digest: name
                .as_ref()
                .map(|value| Sha256::digest(value.as_str()).into()),
        },
        ControlMutation::InsertSeparator { .. }
        | ControlMutation::DeleteItems { .. }
        | ControlMutation::Delete { .. }
        | ControlMutation::MoveItem { .. }
        | ControlMutation::Move { .. }
        | ControlMutation::DuplicateItems { .. } => canonical_item(mutation)?,
        ControlMutation::SplitThought { .. }
        | ControlMutation::ExtractThought { .. }
        | ControlMutation::MergeThoughts { .. }
        | ControlMutation::ExportThoughts { .. }
        | ControlMutation::ReflowThought { .. } => canonical_transform(mutation)?,
        ControlMutation::Replace {
            thought_id,
            expected_digest,
            content,
            ..
        } => FingerprintRequest::Replace {
            thought_id: *thought_id,
            expected_digest: *expected_digest,
            content_digest: Sha256::digest(content.as_bytes()).into(),
        },
        ControlMutation::SetCollapsed {
            thought_id,
            collapsed,
            ..
        } => FingerprintRequest::SetCollapsed {
            thought_id: *thought_id,
            collapsed: *collapsed,
        },
        ControlMutation::History { scope, undo, .. } => FingerprintRequest::History {
            scope: *scope,
            undo: *undo,
        },
        ControlMutation::RenameSession { .. }
        | ControlMutation::Sync
        | ControlMutation::UpdatePrepare { .. }
        | ControlMutation::UpdateRelease { .. }
        | ControlMutation::UpdateQuiesce { .. }
        | ControlMutation::UpdateRestart { .. }
        | ControlMutation::CaptureTakeover { .. } => return Ok(None),
    };
    Ok(Some(request))
}

fn canonical_add(mutation: &ControlMutation) -> Result<FingerprintRequest, StoreError> {
    let (thought_id, content, annotations, name, position, preserve_owned_annotations) =
        match mutation {
            ControlMutation::Add {
                thought_id,
                content,
                annotations,
                position,
                ..
            } => (*thought_id, content, annotations, None, *position, false),
            ControlMutation::PreserveAdd {
                thought_id,
                content,
                annotations,
                name,
                position,
                ..
            } => (
                *thought_id,
                content,
                annotations,
                name.as_ref(),
                *position,
                true,
            ),
            _ => return Err(wrong_fingerprint_family()),
        };
    let payload_digest = if preserve_owned_annotations {
        preserved_payload_digest(content, annotations, name)?
    } else {
        thought_payload_digest_with_name(content, annotations, name)?
    };
    Ok(FingerprintRequest::Add {
        thought_id,
        payload_digest,
        position: optional_index(position)?,
        preserve_owned_annotations,
    })
}

fn preserved_payload_digest(
    content: &str,
    annotations: &[ContentAnnotation],
    name: Option<&ThoughtName>,
) -> Result<[u8; 32], StoreError> {
    let encoded = serde_json::to_vec(&PreservedPayload {
        content,
        annotations,
        name,
    })
    .map_err(|error| StoreError::Serialization(error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(PRESERVED_PAYLOAD_DOMAIN_SEPARATOR);
    digest.update(encoded);
    Ok(digest.finalize().into())
}

fn canonical_item(mutation: &ControlMutation) -> Result<FingerprintRequest, StoreError> {
    Ok(match mutation {
        ControlMutation::InsertSeparator {
            separator_id,
            position,
            ..
        } => FingerprintRequest::InsertSeparator {
            separator_id: *separator_id,
            position: optional_index(*position)?,
        },
        ControlMutation::DeleteItems { item_ids, .. } => FingerprintRequest::DeleteItems {
            item_ids: item_ids.clone(),
        },
        ControlMutation::Delete { thought_id, .. } => FingerprintRequest::DeleteItems {
            item_ids: vec![BoardItemId::Thought(*thought_id)],
        },
        ControlMutation::MoveItem {
            item_id, position, ..
        } => FingerprintRequest::MoveItem {
            item_id: *item_id,
            position: index(*position)?,
        },
        ControlMutation::Move {
            thought_id,
            position,
            ..
        } => FingerprintRequest::MoveItem {
            item_id: BoardItemId::Thought(*thought_id),
            position: index(*position)?,
        },
        ControlMutation::DuplicateItems { item_ids, .. } => FingerprintRequest::DuplicateItems {
            item_ids: item_ids.clone(),
        },
        _ => return Err(wrong_fingerprint_family()),
    })
}

fn canonical_transform(mutation: &ControlMutation) -> Result<FingerprintRequest, StoreError> {
    Ok(match mutation {
        ControlMutation::SplitThought {
            thought_id,
            new_thought_id,
            expected_digest,
            at_byte,
            ..
        } => FingerprintRequest::SplitThought {
            thought_id: *thought_id,
            new_thought_id: *new_thought_id,
            expected_digest: *expected_digest,
            at_byte: index(*at_byte)?,
        },
        ControlMutation::ExtractThought {
            thought_id,
            new_thought_id,
            expected_digest,
            start_byte,
            end_byte,
            ..
        } => FingerprintRequest::ExtractThought {
            thought_id: *thought_id,
            new_thought_id: *new_thought_id,
            expected_digest: *expected_digest,
            start_byte: index(*start_byte)?,
            end_byte: index(*end_byte)?,
        },
        ControlMutation::MergeThoughts {
            thought_ids,
            expected_digests,
            separator,
            ..
        } => FingerprintRequest::MergeThoughts {
            thought_ids: thought_ids.clone(),
            expected_digests: expected_digests.clone(),
            separator_digest: Sha256::digest(separator.as_bytes()).into(),
        },
        ControlMutation::ReflowThought {
            thought_id,
            expected_digest,
            ..
        } => FingerprintRequest::ReflowThought {
            thought_id: *thought_id,
            expected_digest: *expected_digest,
        },
        // Content digests are execution preconditions derived from the current
        // Board, not caller input, so an exact retry after later edits still matches.
        ControlMutation::ExportThoughts {
            thought_ids,
            disposition,
            reference_thought_id,
            output_path,
            ..
        } => FingerprintRequest::ExportThoughts {
            thought_ids: thought_ids.clone(),
            disposition: *disposition,
            reference_thought_id: *reference_thought_id,
            output_path_digest: Sha256::digest(output_path.as_bytes()).into(),
        },
        _ => return Err(wrong_fingerprint_family()),
    })
}

fn wrong_fingerprint_family() -> StoreError {
    StoreError::Serialization("mutation routed to the wrong fingerprint family".to_owned())
}

fn optional_index(value: Option<usize>) -> Result<Option<u64>, StoreError> {
    value.map(index).transpose()
}

fn index(value: usize) -> Result<u64, StoreError> {
    u64::try_from(value)
        .map_err(|_| StoreError::Serialization("request index cannot fit in u64".to_owned()))
}

#[cfg(test)]
mod tests;
