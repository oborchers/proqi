//! Typed, versioned owner-control protocol for active sessions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{BoardItemId, RequestId, SessionId, ThoughtId};

use super::update::{UpdatePrepareReply, UpdateQuiesceReply, UpdateRestartReply};
use super::{runtime::InstanceInfo, store::CommitReceipt};

mod mutation;
pub use mutation::ControlMutation;

/// Current local owner-control protocol.
pub const CONTROL_PROTOCOL_VERSION: u32 = 12;
/// Current compatible screenshot takeover protocol.
pub const CAPTURE_CONTROL_PROTOCOL_VERSION: u32 = 1;
/// Oldest owner-control protocol accepted for plain-text mutations.
pub const MIN_CONTROL_PROTOCOL_VERSION: u32 = 1;
/// Oldest owner-control protocol capable of update coordination.
pub const UPDATE_MUTATION_MINIMUM_PROTOCOL: u32 = 3;
/// Maximum encoded request or response, including framing newline.
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 1_048_576;

/// Whether one advertised owner protocol can represent a mutation family.
#[must_use]
pub const fn control_protocol_supports(protocol: Option<u32>, minimum: u32) -> bool {
    match protocol {
        Some(version) => version >= minimum && version <= CONTROL_PROTOCOL_VERSION,
        None => false,
    }
}

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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlReceipt {
    /// Created or affected thought, when applicable.
    pub thought_id: Option<ThoughtId>,
    /// Created or affected mixed Board items, when applicable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_ids: Vec<BoardItemId>,
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
    /// Prepared participant stopped schema use and released its shared lease.
    Quiesced(UpdateQuiesceReply),
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
