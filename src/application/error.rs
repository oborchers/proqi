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
    /// Action is invalid in the current mode or history state.
    InvalidState,
    /// Clipboard access failed without mutating content.
    ClipboardFailed,
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
            Self::InvalidState => "invalid_state",
            Self::ClipboardFailed => "clipboard_failed",
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
    #[error("revision precondition failed for thought {0}")]
    RevisionConflict(ThoughtId),
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
            Self::RevisionConflict(_) | Self::InvalidState | Self::SequenceExhausted => {
                FailureCode::InvalidState
            }
            Self::Domain(_) => FailureCode::InvariantViolation,
        }
    }
}

/// Application result type.
pub type ApplicationResult<T> = Result<T, ApplicationError>;
