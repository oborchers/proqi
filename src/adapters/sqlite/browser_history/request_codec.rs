//! Versioned session-administration receipts retained beside Browser operations.
//!
//! `browser_operation_receipts` stores either one reversible Browser operation
//! or one tagged request receipt that deliberately creates no Browser history.
//! This module is the single decoder for both payload families.

use std::path::PathBuf;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    domain::{BrowserMutation, BrowserOperation, BrowserOperationKind, OperationId, SessionId},
    ports::store::{SessionRequest, StoreError},
};

use super::codec::decode as decode_operation;
use crate::adapters::sqlite::support::{path_from_bytes, path_to_bytes};

const RENAME_NOOP: &str = "rename_noop_v1";
const TRASH_NOOP: &str = "trash_noop_v1";
const SESSION_CREATE: &str = "session_create_v1";
const SESSION_PRUNE: &str = "session_prune_v1";
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// One retained request receipt that does not move Browser history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::adapters::sqlite) enum RequestReceipt {
    /// A rename whose requested name already matched the session.
    RenameNoOp {
        operation_id: OperationId,
        session_id: SessionId,
        name: Option<String>,
    },
    /// A trash request for a session that was already in trash.
    TrashNoOp {
        operation_id: OperationId,
        session_id: SessionId,
    },
    /// Creation of one named session.
    Create {
        operation_id: OperationId,
        session_id: SessionId,
        name: String,
        origin_cwd: PathBuf,
    },
    /// Permanent deletion of one trashed session.
    Prune {
        operation_id: OperationId,
        session_id: SessionId,
    },
}

/// Any payload retained for one Browser request identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::adapters::sqlite) enum RetainedPayload {
    /// Reversible Browser history entry.
    Operation(BrowserOperation),
    /// Receipt without a Browser history entry.
    Receipt(RequestReceipt),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RenameNoOpJson {
    receipt: String,
    operation_id: OperationId,
    session_id: SessionId,
    name: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SessionOnlyJson {
    receipt: String,
    operation_id: OperationId,
    session_id: SessionId,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateJson {
    receipt: String,
    operation_id: OperationId,
    session_id: SessionId,
    name: String,
    origin_cwd_hex: String,
}

impl RequestReceipt {
    /// Identity that owns this receipt.
    pub(in crate::adapters::sqlite) const fn operation_id(&self) -> OperationId {
        match self {
            Self::RenameNoOp { operation_id, .. }
            | Self::TrashNoOp { operation_id, .. }
            | Self::Create { operation_id, .. }
            | Self::Prune { operation_id, .. } => *operation_id,
        }
    }

    /// Session addressed by this receipt.
    pub(in crate::adapters::sqlite) const fn session_id(&self) -> SessionId {
        match self {
            Self::RenameNoOp { session_id, .. }
            | Self::TrashNoOp { session_id, .. }
            | Self::Create { session_id, .. }
            | Self::Prune { session_id, .. } => *session_id,
        }
    }

    /// Stable versioned JSON representation.
    pub(in crate::adapters::sqlite) fn encode(&self) -> Result<String, StoreError> {
        let encoded = match self {
            Self::RenameNoOp {
                operation_id,
                session_id,
                name,
            } => serde_json::to_string(&RenameNoOpJson {
                receipt: RENAME_NOOP.to_owned(),
                operation_id: *operation_id,
                session_id: *session_id,
                name: name.clone(),
            }),
            Self::TrashNoOp {
                operation_id,
                session_id,
            } => serde_json::to_string(&session_only(TRASH_NOOP, *operation_id, *session_id)),
            Self::Prune {
                operation_id,
                session_id,
            } => serde_json::to_string(&session_only(SESSION_PRUNE, *operation_id, *session_id)),
            Self::Create {
                operation_id,
                session_id,
                name,
                origin_cwd,
            } => serde_json::to_string(&CreateJson {
                receipt: SESSION_CREATE.to_owned(),
                operation_id: *operation_id,
                session_id: *session_id,
                name: name.clone(),
                origin_cwd_hex: encode_hex(&path_to_bytes(origin_cwd)),
            }),
        };
        encoded.map_err(|_| StoreError::Serialization("session receipt encoding failed".to_owned()))
    }

    fn request(&self) -> SessionRequest {
        match self {
            Self::RenameNoOp {
                session_id, name, ..
            } => SessionRequest::Rename {
                session_id: *session_id,
                name: name.clone(),
            },
            Self::TrashNoOp { session_id, .. } => SessionRequest::Trash {
                session_id: *session_id,
            },
            Self::Create {
                session_id,
                name,
                origin_cwd,
                ..
            } => SessionRequest::Create {
                session_id: *session_id,
                name: name.clone(),
                origin_cwd: origin_cwd.clone(),
            },
            Self::Prune { session_id, .. } => SessionRequest::Prune {
                session_id: *session_id,
            },
        }
    }
}

impl RetainedPayload {
    /// Semantic request represented by this payload.
    pub(in crate::adapters::sqlite) fn request(&self) -> Result<SessionRequest, StoreError> {
        match self {
            Self::Receipt(receipt) => Ok(receipt.request()),
            Self::Operation(operation) => operation_request(operation),
        }
    }

    const fn identity(&self) -> (OperationId, SessionId) {
        match self {
            Self::Receipt(receipt) => (receipt.operation_id(), receipt.session_id()),
            Self::Operation(operation) => (operation.id(), operation.session_id()),
        }
    }
}

/// Decode one retained payload and verify its row metadata.
pub(in crate::adapters::sqlite) fn decode_retained(
    payload: &str,
    operation_id: OperationId,
    target_session: &[u8],
) -> Result<RetainedPayload, StoreError> {
    let retained = decode_payload(payload)?;
    let (stored_id, stored_session) = retained.identity();
    if stored_id != operation_id || stored_session.database_bytes().as_slice() != target_session {
        return Err(StoreError::Corrupt(
            "Browser operation receipt metadata does not match its payload".to_owned(),
        ));
    }
    Ok(retained)
}

/// Whether a retained payload must outlive the permanent prune of its session.
pub(in crate::adapters::sqlite) fn survives_session_prune(
    payload: &str,
) -> Result<bool, StoreError> {
    Ok(matches!(
        decode_payload(payload)?,
        RetainedPayload::Receipt(RequestReceipt::Create { .. } | RequestReceipt::Prune { .. })
    ))
}

fn decode_payload(payload: &str) -> Result<RetainedPayload, StoreError> {
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| StoreError::Corrupt("invalid Browser operation receipt".to_owned()))?;
    let Some(tag) = value.get("receipt") else {
        return decode_operation(payload).map(RetainedPayload::Operation);
    };
    let receipt = match tag.as_str() {
        Some(RENAME_NOOP) => {
            let json: RenameNoOpJson = decode_json(value)?;
            RequestReceipt::RenameNoOp {
                operation_id: json.operation_id,
                session_id: json.session_id,
                name: json.name,
            }
        }
        Some(TRASH_NOOP) => {
            let json: SessionOnlyJson = decode_json(value)?;
            RequestReceipt::TrashNoOp {
                operation_id: json.operation_id,
                session_id: json.session_id,
            }
        }
        Some(SESSION_PRUNE) => {
            let json: SessionOnlyJson = decode_json(value)?;
            RequestReceipt::Prune {
                operation_id: json.operation_id,
                session_id: json.session_id,
            }
        }
        Some(SESSION_CREATE) => {
            let json: CreateJson = decode_json(value)?;
            RequestReceipt::Create {
                operation_id: json.operation_id,
                session_id: json.session_id,
                name: json.name,
                origin_cwd: path_from_bytes(decode_hex(&json.origin_cwd_hex)?)?,
            }
        }
        _ => {
            return Err(StoreError::Corrupt(
                "unknown session request receipt".to_owned(),
            ));
        }
    };
    Ok(RetainedPayload::Receipt(receipt))
}

fn operation_request(operation: &BrowserOperation) -> Result<SessionRequest, StoreError> {
    let session_id = operation.session_id();
    match (operation.kind(), operation.forward()) {
        (BrowserOperationKind::Rename, BrowserMutation::SetName { value, .. }) => {
            Ok(SessionRequest::Rename {
                session_id,
                name: value.clone(),
            })
        }
        (BrowserOperationKind::Trash, BrowserMutation::SetDeletedAt { .. }) => {
            Ok(SessionRequest::Trash { session_id })
        }
        (BrowserOperationKind::Restore, BrowserMutation::SetDeletedAt { .. }) => {
            Ok(SessionRequest::Restore { session_id })
        }
        _ => Err(StoreError::Corrupt(
            "Browser operation kind does not match its mutation".to_owned(),
        )),
    }
}

fn session_only(
    receipt: &str,
    operation_id: OperationId,
    session_id: SessionId,
) -> SessionOnlyJson {
    SessionOnlyJson {
        receipt: receipt.to_owned(),
        operation_id,
        session_id,
    }
}

fn decode_json<T: DeserializeOwned>(value: Value) -> Result<T, StoreError> {
    serde_json::from_value(value)
        .map_err(|_| StoreError::Corrupt("invalid session request receipt".to_owned()))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, StoreError> {
    let invalid = || StoreError::Corrupt("invalid session receipt path encoding".to_owned());
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(invalid());
    }
    let digit = |byte: u8| {
        HEX_DIGITS
            .iter()
            .position(|candidate| *candidate == byte)
            .and_then(|position| u8::try_from(position).ok())
    };
    pairs
        .iter()
        .map(|[high, low]| match (digit(*high), digit(*low)) {
            (Some(high), Some(low)) => Ok(high << 4 | low),
            _ => Err(invalid()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{RequestReceipt, RetainedPayload, decode_hex, decode_retained, encode_hex};
    use crate::ports::store::SessionRequest;

    #[test]
    fn every_receipt_kind_round_trips_with_its_semantic_request() {
        let operation_id = "op_06g30t8fudrq55fdkjqr6mpe44".parse().expect("operation");
        let session_id = "ses_06g30t7dv5qv55n1ppn3clis3k".parse().expect("session");
        let receipts = [
            RequestReceipt::RenameNoOp {
                operation_id,
                session_id,
                name: Some("ünïcödé".to_owned()),
            },
            RequestReceipt::TrashNoOp {
                operation_id,
                session_id,
            },
            RequestReceipt::Prune {
                operation_id,
                session_id,
            },
            RequestReceipt::Create {
                operation_id,
                session_id,
                name: "agent-os-claude".to_owned(),
                origin_cwd: PathBuf::from("/work/ü space"),
            },
        ];
        for receipt in receipts {
            let payload = receipt.encode().expect("encode");
            let decoded = decode_retained(&payload, operation_id, &session_id.database_bytes())
                .expect("decode");
            assert_eq!(decoded, RetainedPayload::Receipt(receipt.clone()));
            let request = decoded.request().expect("request");
            assert!(matches!(
                request,
                SessionRequest::Rename { .. }
                    | SessionRequest::Trash { .. }
                    | SessionRequest::Prune { .. }
                    | SessionRequest::Create { .. }
            ));
        }
    }

    #[test]
    fn row_metadata_mismatch_and_unknown_tags_fail_closed() {
        let operation_id = "op_06g30t8fudrq55fdkjqr6mpe44".parse().expect("operation");
        let session_id: crate::domain::SessionId =
            "ses_06g30t7dv5qv55n1ppn3clis3k".parse().expect("session");
        let payload = RequestReceipt::TrashNoOp {
            operation_id,
            session_id,
        }
        .encode()
        .expect("encode");
        assert!(decode_retained(&payload, operation_id, &[0; 16]).is_err());
        let unknown = payload.replace("trash_noop_v1", "future_receipt_v9");
        assert!(decode_retained(&unknown, operation_id, &session_id.database_bytes()).is_err());
        let extra = payload.replace('}', ",\"extra\":1}");
        assert!(decode_retained(&extra, operation_id, &session_id.database_bytes()).is_err());
    }

    #[test]
    fn path_hex_is_lossless_and_strict() {
        let bytes = [0_u8, 1, 0x7f, 0x80, 0xff];
        assert_eq!(decode_hex(&encode_hex(&bytes)).expect("hex"), bytes);
        assert!(decode_hex("abc").is_err());
        assert!(decode_hex("zz").is_err());
        assert!(decode_hex("AB").is_err());
    }
}
