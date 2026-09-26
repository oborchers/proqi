//! Board completion of a plain-text thought export whose file is already durable.

use std::path::{Path, PathBuf};

use super::{AppState, ApplicationError, ApplicationResult, exact_live_thought};
use crate::{
    domain::{ExportDisposition, OperationId, Thought, ThoughtId, Timestamp},
    ports::control::ControlMutation,
};

/// Board change applied only after the exported file was written and synchronized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportBoardChange {
    /// Recoverably delete the exported thoughts.
    Remove,
    /// Replace the exported thoughts with one file-reference thought.
    ReplaceWithReference {
        /// Identity of the new reference thought.
        reference_thought_id: ThoughtId,
        /// Absolute path of the durable exported file.
        path: PathBuf,
    },
}

/// One exact export completion: sources must still match the exported snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportCompletion {
    /// Durable operation identity.
    pub operation_id: OperationId,
    /// Exported thoughts in Board order.
    pub thought_ids: Vec<ThoughtId>,
    /// Exact snapshots whose copy text was written to the file.
    pub expected_sources: Vec<Thought>,
    /// Removal or replacement.
    pub change: ExportBoardChange,
    /// Event time.
    pub at: Timestamp,
}

/// Deterministic reference identity for one export operation, so replays resolve exactly.
///
/// # Errors
///
/// Returns an invalid-state error when the operation bytes cannot form a thought identity.
pub fn export_reference_thought_id(operation_id: OperationId) -> ApplicationResult<ThoughtId> {
    ThoughtId::from_database_bytes(operation_id.database_bytes())
        .map_err(|_| ApplicationError::InvalidState)
}

/// Build the exact completion for a forwarded or inactive-session export request.
///
/// Every source must be live and match its expected SHA-256 content digest. A
/// replacement must carry the operation-derived reference identity and an
/// absolute path.
///
/// # Errors
///
/// Returns a typed missing, conflict, or invalid-state error before any mutation.
pub fn export_completion_for_request(
    state: &AppState,
    request: &ControlMutation,
    at: Timestamp,
) -> ApplicationResult<ExportCompletion> {
    let ControlMutation::ExportThoughts {
        operation_id,
        thought_ids,
        expected_digests,
        disposition,
        reference_thought_id,
        output_path,
    } = request
    else {
        return Err(ApplicationError::InvalidState);
    };
    if thought_ids.is_empty() || thought_ids.len() != expected_digests.len() {
        return Err(ApplicationError::InvalidState);
    }
    if !Path::new(output_path).is_absolute() {
        return Err(ApplicationError::InvalidState);
    }
    let change = match (disposition, reference_thought_id) {
        (ExportDisposition::Remove, None) => ExportBoardChange::Remove,
        (ExportDisposition::ReplaceWithReference, Some(reference))
            if *reference == export_reference_thought_id(*operation_id)? =>
        {
            ExportBoardChange::ReplaceWithReference {
                reference_thought_id: *reference,
                path: PathBuf::from(output_path),
            }
        }
        _ => return Err(ApplicationError::InvalidState),
    };
    let expected_sources = thought_ids
        .iter()
        .zip(expected_digests)
        .map(|(id, digest)| exact_live_thought(state, *id, Some(*digest)).cloned())
        .collect::<ApplicationResult<Vec<_>>>()?;
    Ok(ExportCompletion {
        operation_id: *operation_id,
        thought_ids: thought_ids.clone(),
        expected_sources,
        change,
        at,
    })
}
