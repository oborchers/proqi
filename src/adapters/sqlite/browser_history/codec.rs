//! Durable Browser operation encoding and kind metadata.

use crate::{
    domain::{BrowserOperation, BrowserOperationKind},
    ports::store::StoreError,
};

pub(super) fn encode(operation: &BrowserOperation) -> Result<String, StoreError> {
    serde_json::to_string(operation).map_err(|error| StoreError::Serialization(error.to_string()))
}

pub(super) fn decode(payload: &str) -> Result<BrowserOperation, StoreError> {
    let operation: BrowserOperation =
        serde_json::from_str(payload).map_err(|error| StoreError::Corrupt(error.to_string()))?;
    operation
        .validate()
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    Ok(operation)
}

pub(super) const fn kind_str(kind: BrowserOperationKind) -> &'static str {
    match kind {
        BrowserOperationKind::Rename => "rename",
        BrowserOperationKind::Trash => "trash",
        BrowserOperationKind::Restore => "restore",
    }
}

pub(super) fn parse_kind(value: &str) -> Result<BrowserOperationKind, StoreError> {
    match value {
        "rename" => Ok(BrowserOperationKind::Rename),
        "trash" => Ok(BrowserOperationKind::Trash),
        "restore" => Ok(BrowserOperationKind::Restore),
        _ => Err(StoreError::Corrupt(
            "unknown Browser operation kind".to_owned(),
        )),
    }
}
