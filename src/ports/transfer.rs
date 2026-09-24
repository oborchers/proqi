//! Terminal-independent cross-session thought delivery request.

use crate::domain::{ContentAnnotation, OperationId, SessionId, ThoughtId, ThoughtName};
use serde::{Deserialize, Serialize};

/// One exact source snapshot and its stable destination identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransferItem {
    /// Source thought retained until the whole destination cohort is durable.
    pub source_thought_id: ThoughtId,
    /// New destination thought identity.
    pub destination_thought_id: ThoughtId,
    /// Exact canonical body.
    pub content: String,
    /// Exact semantic presentation metadata; destination ordinals are allocated afresh.
    pub annotations: Vec<ContentAnnotation>,
    /// Optional organizational name.
    pub name: Option<ThoughtName>,
}

/// One selected transfer whose destination copy is committed atomically.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionTransferBatchRequest {
    /// Exact source session.
    pub source_session_id: SessionId,
    /// Selected destination session.
    pub destination_session_id: SessionId,
    /// Stable destination cohort operation identity.
    pub operation_id: OperationId,
    /// Stable source removal operation identity, even for a keep transfer.
    pub removal_operation_id: OperationId,
    /// Exact selected source order, excluding separators.
    pub items: Vec<TransferItem>,
    /// Whether to remove all unchanged sources after cohort acceptance.
    pub remove_source: bool,
}
