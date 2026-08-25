//! Typed, versioned owner-control protocol for active sessions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{ContentAnnotation, OperationId, RequestId, SessionId, ThoughtId, UndoScope};

use super::update::{
    UpdatePrepareReply, UpdatePrepareRequest, UpdateRestartReply, UpdateRestartRequest,
};
use super::{runtime::InstanceInfo, store::CommitReceipt};

/// Current local owner-control protocol.
pub const CONTROL_PROTOCOL_VERSION: u32 = 3;
/// Oldest owner-control protocol accepted for plain-text mutations.
pub const MIN_CONTROL_PROTOCOL_VERSION: u32 = 1;
/// Maximum encoded request or response, including framing newline.
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 1_048_576;

/// One mutation routed to the process owning a session reducer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mutation")]
pub enum ControlMutation {
    /// Create one exact-content thought.
    Add {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Deterministic thought identity associated with this request.
        thought_id: ThoughtId,
        /// Exact content, including line endings.
        content: String,
        /// Durable presentation metadata, available from protocol version 2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<ContentAnnotation>,
        /// Optional zero-based insertion position.
        position: Option<usize>,
    },
    /// Soft-delete one thought.
    Delete {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to delete.
        thought_id: ThoughtId,
    },
    /// Move one thought.
    Move {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to move.
        thought_id: ThoughtId,
        /// Zero-based target position.
        position: usize,
    },
    /// Persistently move one history scope.
    History {
        /// Durable history operation identity.
        operation_id: OperationId,
        /// Board or editor scope.
        scope: UndoScope,
        /// Undo when true, redo otherwise.
        undo: bool,
    },
    /// Ask one live owner to flush and enter a bounded update barrier.
    UpdatePrepare {
        /// Shared all-session readiness request.
        request: UpdatePrepareRequest,
    },
    /// Release a previously prepared owner after cancellation or failure.
    UpdateRelease {
        /// Shared attempt identity.
        operation_id: RequestId,
    },
    /// Ask one prepared owner to clean up and replace itself.
    UpdateRestart {
        /// Verified installed version and shared attempt identity.
        request: UpdateRestartRequest,
    },
}

impl ControlMutation {
    /// Durable idempotency identity carried by every mutation.
    #[must_use]
    pub const fn durable_operation_id(&self) -> Option<OperationId> {
        match self {
            Self::Add { operation_id, .. }
            | Self::Delete { operation_id, .. }
            | Self::Move { operation_id, .. }
            | Self::History { operation_id, .. } => Some(*operation_id),
            Self::UpdatePrepare { .. }
            | Self::UpdateRelease { .. }
            | Self::UpdateRestart { .. } => None,
        }
    }

    /// Thought affected by this request, when applicable.
    #[must_use]
    pub const fn thought_id(&self) -> Option<ThoughtId> {
        match self {
            Self::Add { thought_id, .. }
            | Self::Delete { thought_id, .. }
            | Self::Move { thought_id, .. } => Some(*thought_id),
            Self::History { .. }
            | Self::UpdatePrepare { .. }
            | Self::UpdateRelease { .. }
            | Self::UpdateRestart { .. } => None,
        }
    }

    /// Whether this mutation requires the annotation-aware protocol.
    #[must_use]
    pub fn requires_protocol_two(&self) -> bool {
        matches!(self, Self::Add { annotations, .. } if !annotations.is_empty())
    }

    /// Oldest control protocol capable of representing this request.
    #[must_use]
    pub fn minimum_protocol(&self) -> u32 {
        if matches!(
            self,
            Self::UpdatePrepare { .. } | Self::UpdateRelease { .. } | Self::UpdateRestart { .. }
        ) {
            3
        } else if self.requires_protocol_two() {
            2
        } else {
            1
        }
    }
}

/// One bounded request addressed to an exact session owner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlRequest {
    /// Protocol offered by the client.
    pub protocol: u32,
    /// Idempotency identity for this local transport request.
    pub request_id: RequestId,
    /// Exact session being mutated.
    pub session_id: SessionId,
    /// Typed mutation payload.
    pub mutation: ControlMutation,
}

/// Accepted durable operation returned by the owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlReceipt {
    /// Created or affected thought, when applicable.
    pub thought_id: Option<ThoughtId>,
    /// Store-confirmed durable operation receipt.
    pub durable: CommitReceipt,
}

/// Versioned response from the verified owner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlResponse {
    /// Protocol selected by the owner.
    pub protocol: u32,
    /// Request this response answers.
    pub request_id: RequestId,
    /// Accepted receipt or typed rejection.
    pub result: ControlResult,
}

/// Owner outcome without transport ambiguity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ControlResult {
    /// Mutation became durable through the owner reducer and store lane.
    Accepted(ControlReceipt),
    /// Ephemeral update readiness or restart receipt.
    Update(ControlUpdateReceipt),
    /// Owner rejected the mutation without reporting it as durable.
    Rejected {
        /// Stable error code.
        code: String,
        /// Human-readable explanation without thought content.
        message: String,
    },
}

/// Successful typed result from update coordination over the owner endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "update")]
pub enum ControlUpdateReceipt {
    /// Readiness result after durable flushing.
    Prepared(UpdatePrepareReply),
    /// Prepared participant returned to normal use.
    Released {
        /// Participant acknowledging release.
        instance_id: crate::domain::InstanceId,
    },
    /// Participant accepted or rejected replacement responsibility.
    Restart(UpdateRestartReply),
}

/// Local transport or verification failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ControlError {
    /// Active owner does not advertise the required protocol or endpoint.
    #[error("active owner does not support verified control forwarding")]
    Unsupported,
    /// Peer credentials do not match the advertised owner.
    #[error("control peer identity could not be verified")]
    InvalidPeer,
    /// Request or response exceeded the protocol bound.
    #[error("control message exceeds the protocol limit")]
    MessageTooLarge,
    /// Protocol versions or request identities do not match.
    #[error("control protocol mismatch: {0}")]
    Protocol(String),
    /// Owner did not respond before the bounded deadline.
    #[error("control request timed out")]
    Timeout,
    /// Local transport failed.
    #[error("control transport failed: {0}")]
    Io(String),
    /// Owner explicitly rejected the typed mutation.
    #[error("control mutation rejected ({code}): {message}")]
    Rejected {
        /// Stable owner error code.
        code: String,
        /// Redacted owner explanation.
        message: String,
    },
}

/// Client facade used by scriptable commands.
pub trait ControlClient {
    /// Send one request only to its verified active owner.
    ///
    /// # Errors
    ///
    /// Returns a typed verification, timeout, protocol, transport, or owner failure.
    fn send(
        &self,
        owner: &InstanceInfo,
        request: &ControlRequest,
    ) -> Result<ControlReceipt, ControlError>;
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{ContentAnnotation, ContentAnnotationKind},
        ports::environment::IdGenerator,
    };

    use super::{ControlMutation, ControlRequest};

    #[test]
    fn plain_v1_add_stays_wire_compatible_and_annotations_require_v2() {
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let plain = ControlRequest {
            protocol: 1,
            request_id: ids.request_id(),
            session_id: ids.session_id(),
            mutation: ControlMutation::Add {
                operation_id: ids.operation_id(),
                thought_id: ids.thought_id(),
                content: "plain".to_owned(),
                annotations: Vec::new(),
                position: None,
            },
        };
        let encoded = serde_json::to_string(&plain).expect("serialize v1 request");
        assert!(!encoded.contains("annotations"));
        let decoded: ControlRequest = serde_json::from_str(&encoded).expect("deserialize v1");
        assert_eq!(decoded, plain);
        assert!(!decoded.mutation.requires_protocol_two());

        let annotated = ControlMutation::Add {
            operation_id: ids.operation_id(),
            thought_id: ids.thought_id(),
            content: "/tmp/a.png".to_owned(),
            annotations: vec![ContentAnnotation {
                start: 0,
                end: 10,
                kind: ContentAnnotationKind::Attachment {
                    image: true,
                    display_name: "a.png".to_owned(),
                },
            }],
            position: None,
        };
        assert!(annotated.requires_protocol_two());
    }
}
