//! Typed, versioned owner-control protocol for active sessions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    ContentAnnotation, ContentAnnotationKind, OperationId, RequestId, RevisionId, SessionId,
    ThoughtId, UndoScope,
};

use super::store::DurableIdentity;
use super::update::{
    UpdatePrepareReply, UpdatePrepareRequest, UpdateRestartReply, UpdateRestartRequest,
};
use super::{runtime::InstanceInfo, store::CommitReceipt};

/// Current local owner-control protocol.
pub const CONTROL_PROTOCOL_VERSION: u32 = 7;
/// Current compatible screenshot takeover protocol.
pub const CAPTURE_CONTROL_PROTOCOL_VERSION: u32 = 1;
/// Oldest owner-control protocol accepted for plain-text mutations.
pub const MIN_CONTROL_PROTOCOL_VERSION: u32 = 1;
/// Maximum encoded request or response, including framing newline.
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 1_048_576;

/// Stable rejection codes emitted by the local owner-control protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlRejectionCode {
    /// Client and owner cannot negotiate a protocol version.
    ProtocolMismatch,
    /// One request identity was reused with a different payload.
    RequestIdConflict,
    /// Owner cannot admit another request without exceeding its bound.
    OwnerBusy,
    /// The external caller cannot know whether the operation completed.
    OutcomeUnknown,
    /// The active owner has begun shutting down.
    OwnerShuttingDown,
    /// The request addresses a different active session.
    WrongSession,
    /// A request reached a lane that cannot represent it.
    InvalidControlRequest,
    /// Another update already owns the preparation barrier.
    AnotherUpdateIsPreparing,
    /// Update release does not match the active operation.
    UpdateOperationMismatch,
    /// A durable operation identity was reused with a different mutation.
    IdempotencyConflict,
    /// The requested mutation did not produce a durable change.
    NoDurableMutation,
    /// The durable storage operation failed.
    StorageFailed,
    /// The takeover request did not name the current capture owner.
    CaptureOwnerMismatch,
    /// This process no longer owns the screenshot inbox.
    CaptureNotOwned,
    /// Another screenshot takeover is already draining.
    CaptureTakeoverInProgress,
}

impl ControlRejectionCode {
    /// Stable machine-readable representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProtocolMismatch => "protocol_mismatch",
            Self::RequestIdConflict => "request_id_conflict",
            Self::OwnerBusy => "owner_busy",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::OwnerShuttingDown => "owner_shutting_down",
            Self::WrongSession => "wrong_session",
            Self::InvalidControlRequest => "invalid_control_request",
            Self::AnotherUpdateIsPreparing => "another_update_is_preparing",
            Self::UpdateOperationMismatch => "update_operation_mismatch",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::NoDurableMutation => "no_durable_mutation",
            Self::StorageFailed => "storage_failed",
            Self::CaptureOwnerMismatch => "capture_owner_mismatch",
            Self::CaptureNotOwned => "capture_not_owned",
            Self::CaptureTakeoverInProgress => "capture_takeover_in_progress",
        }
    }
}

/// One mutation routed to the process owning a session reducer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mutation")]
pub enum ControlMutation {
    /// Rename or clear the active session through its owner.
    RenameSession {
        /// Replacement name, or `None` to clear it.
        name: Option<String>,
    },
    /// Flush pending editor work before an active-session CLI read.
    Sync,
    /// Replace exact thought content as one persistent editor revision.
    Replace {
        /// Durable editor revision identity.
        revision_id: RevisionId,
        /// Thought to replace.
        thought_id: ThoughtId,
        /// Required SHA-256 of current content, omitted only for explicit force.
        expected_digest: Option<[u8; 32]>,
        /// Exact replacement content.
        content: String,
    },
    /// Set one thought's durable collapse state.
    SetCollapsed {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to update.
        thought_id: ThoughtId,
        /// Exact replacement state.
        collapsed: bool,
    },
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
    /// Preserve one already-valid Proqi thought during cross-session transfer.
    PreserveAdd {
        /// Durable destination operation identity.
        operation_id: OperationId,
        /// Deterministic destination thought identity.
        thought_id: ThoughtId,
        /// Exact canonical source content.
        content: String,
        /// Existing validated presentation metadata preserved without re-authoring it.
        annotations: Vec<ContentAnnotation>,
        /// Optional zero-based destination position.
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
    /// Ask the exact live screenshot owner to schedule a verified graceful handoff.
    CaptureTakeover {
        /// Owner identity observed with the authoritative lock contention.
        expected_owner_instance_id: crate::domain::InstanceId,
        /// Process that will retry the authoritative lock.
        requester_instance_id: crate::domain::InstanceId,
        /// Screenshot takeover protocol required by the requester.
        capture_protocol: u32,
    },
}

impl ControlMutation {
    /// Durable idempotency identity carried by every mutation.
    #[must_use]
    pub const fn durable_operation_id(&self) -> Option<OperationId> {
        match self {
            Self::Add { operation_id, .. }
            | Self::PreserveAdd { operation_id, .. }
            | Self::Delete { operation_id, .. }
            | Self::Move { operation_id, .. }
            | Self::History { operation_id, .. }
            | Self::SetCollapsed { operation_id, .. } => Some(*operation_id),
            Self::UpdatePrepare { .. }
            | Self::RenameSession { .. }
            | Self::Sync
            | Self::Replace { .. }
            | Self::UpdateRelease { .. }
            | Self::UpdateRestart { .. }
            | Self::CaptureTakeover { .. } => None,
        }
    }

    /// Durable idempotency identity carried by a mutation.
    #[must_use]
    pub const fn durable_identity(&self) -> Option<DurableIdentity> {
        match self {
            Self::Replace { revision_id, .. } => Some(DurableIdentity::Revision(*revision_id)),
            _ => match self.durable_operation_id() {
                Some(operation_id) => Some(DurableIdentity::Operation(operation_id)),
                None => None,
            },
        }
    }

    /// Thought affected by this request, when applicable.
    #[must_use]
    pub const fn thought_id(&self) -> Option<ThoughtId> {
        match self {
            Self::Add { thought_id, .. }
            | Self::PreserveAdd { thought_id, .. }
            | Self::Delete { thought_id, .. }
            | Self::Move { thought_id, .. } => Some(*thought_id),
            Self::Replace { thought_id, .. } | Self::SetCollapsed { thought_id, .. } => {
                Some(*thought_id)
            }
            Self::History { .. }
            | Self::RenameSession { .. }
            | Self::Sync
            | Self::UpdatePrepare { .. }
            | Self::UpdateRelease { .. }
            | Self::UpdateRestart { .. }
            | Self::CaptureTakeover { .. } => None,
        }
    }

    /// Whether this mutation requires the annotation-aware protocol.
    #[must_use]
    pub fn requires_protocol_two(&self) -> bool {
        matches!(self, Self::Add { annotations, .. } if !annotations.is_empty())
    }

    /// Whether this mutation carries the invocation-reference annotation added in protocol six.
    #[must_use]
    pub fn requires_protocol_six(&self) -> bool {
        matches!(self, Self::Add { annotations, .. } if annotations.iter().any(|annotation| {
            matches!(annotation.kind, ContentAnnotationKind::InvocationReference { .. })
        }))
    }

    /// Whether this purpose-specific request preserves semantic inline metadata.
    #[must_use]
    pub fn requires_protocol_seven(&self) -> bool {
        matches!(self, Self::PreserveAdd { .. })
    }

    /// Oldest control protocol capable of representing this request.
    #[must_use]
    pub fn minimum_protocol(&self) -> u32 {
        if self.requires_protocol_seven() {
            7
        } else if self.requires_protocol_six() {
            6
        } else if matches!(self, Self::CaptureTakeover { .. }) {
            5
        } else if matches!(
            self,
            Self::RenameSession { .. }
                | Self::Replace { .. }
                | Self::SetCollapsed { .. }
                | Self::Sync
        ) {
            4
        } else if matches!(
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
    /// Durable metadata change without an operation sequence.
    Metadata(ControlMetadataReceipt),
    /// Ephemeral screenshot takeover scheduling result.
    Capture(ControlCaptureReceipt),
    /// Owner rejected the mutation without reporting it as durable.
    Rejected {
        /// Stable error code.
        code: String,
        /// Human-readable explanation without thought content.
        message: String,
    },
}

/// Successful screenshot owner-control result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "capture")]
pub enum ControlCaptureReceipt {
    /// The old owner confirmed the response and will drain and release its lock.
    TakeoverScheduled {
        /// Exact owner that accepted relinquishment.
        owner_instance_id: crate::domain::InstanceId,
    },
}

/// Successful metadata result from the active owner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "metadata")]
pub enum ControlMetadataReceipt {
    /// The session name is durable and visible in the owner.
    SessionRenamed {
        /// Durable replacement name.
        name: Option<String>,
    },
    /// All owner work admitted before the request is durable.
    Synchronized,
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
#[path = "control/tests.rs"]
mod tests;
