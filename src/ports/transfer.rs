//! Terminal-independent cross-session thought delivery request.

use crate::domain::{ContentAnnotation, OperationId, SessionId, ThoughtId};

/// Exact thought copy addressed to another Proqi session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTransferRequest {
    /// Destination session selected by the user.
    pub destination_session_id: SessionId,
    /// Source thought retained unless a later local delete is committed.
    pub source_thought_id: ThoughtId,
    /// Durable destination operation identity.
    pub operation_id: OperationId,
    /// Exact canonical thought content.
    pub content: String,
    /// Durable presentation annotations over the canonical content.
    pub annotations: Vec<ContentAnnotation>,
    /// Whether the source should be deleted after destination durability.
    pub remove_source: bool,
}
