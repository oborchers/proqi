//! Stable application errors and notification codes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{DomainError, ThoughtId};

/// Stable application error and notification codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FailureCode {
    /// Requested thought is absent or deleted.
    ThoughtNotFound,
    /// Exact replacement precondition no longer matches current content.
    ContentConflict,
    /// A thought is locked by an in-flight submission.
    ThoughtLocked,
    /// Action is invalid in the current mode or history state.
    InvalidState,
    /// Clipboard access failed without mutating content.
    ClipboardFailed,
    /// Typed clipboard metadata cannot be safely represented on this platform.
    ClipboardMetadataUnsupported,
    /// Persistence failed and state is not yet durable.
    StorageFailed,
    /// Persistence failed and the exact failed batch could not be retained for retry.
    RecoveryCapacity,
    /// Domain invariants rejected the operation.
    InvariantViolation,
}

impl FailureCode {
    /// Return the stable machine-readable spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThoughtNotFound => "thought_not_found",
            Self::ContentConflict => "content_conflict",
            Self::ThoughtLocked => "thought_locked",
            Self::InvalidState => "invalid_state",
            Self::ClipboardFailed => "clipboard_failed",
            Self::ClipboardMetadataUnsupported => "clipboard_metadata_unsupported",
            Self::StorageFailed => "storage_failed",
            Self::RecoveryCapacity => "recovery_capacity",
            Self::InvariantViolation => "invariant_violation",
        }
    }
}

/// Typed application failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ApplicationError {
    /// Domain validation failed.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// Referenced thought is unavailable.
    #[error("live thought not found: {0}")]
    ThoughtNotFound(ThoughtId),
    /// Revision does not match current content or ownership.
    #[error(
        "thought changed since this editor revision; exit edit to undo newer board operations first: {0}"
    )]
    RevisionConflict(ThoughtId),
    /// Exact replacement digest no longer matches current content.
    #[error("content precondition failed for thought {0}")]
    ContentConflict(ThoughtId),
    /// Mutation is forbidden during an in-flight submission.
    #[error("thought has a submission in progress: {0}")]
    ThoughtLocked(ThoughtId),
    /// A board selection does not describe one contiguous ordered range.
    #[error("thought selection must be contiguous and in board order")]
    NoncontiguousSelection,
    /// An action requires another interaction state.
    #[error("action is invalid in the current application state")]
    InvalidState,
    /// Operation sequence cannot increase.
    #[error("operation sequence exhausted")]
    SequenceExhausted,
}

impl ApplicationError {
    /// Return the stable machine-readable classification.
    #[must_use]
    pub const fn code(&self) -> FailureCode {
        match self {
            Self::ThoughtNotFound(_) => FailureCode::ThoughtNotFound,
            Self::ContentConflict(_) => FailureCode::ContentConflict,
            Self::ThoughtLocked(_) => FailureCode::ThoughtLocked,
            Self::RevisionConflict(_)
            | Self::NoncontiguousSelection
            | Self::InvalidState
            | Self::SequenceExhausted => FailureCode::InvalidState,
            Self::Domain(_) => FailureCode::InvariantViolation,
        }
    }
}

/// Application result type.
pub type ApplicationResult<T> = Result<T, ApplicationError>;
