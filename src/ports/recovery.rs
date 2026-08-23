//! Safe filesystem export boundary for unsaved in-memory state.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{OperationSequence, RequestId, Session, Thought, Timestamp};

/// Versioned exact-content recovery document written outside SQLite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryDocument {
    /// Recovery format version.
    pub format_version: u32,
    /// Time at which this in-memory state was captured.
    pub exported_at: Timestamp,
    /// Session metadata as seen by the reducer.
    pub session: Session,
    /// Current live and recoverably deleted thoughts.
    pub thoughts: Vec<Thought>,
    /// Sequences not yet acknowledged as durable.
    pub pending_sequences: Vec<OperationSequence>,
    /// Failed sequence that prompted recovery, if any.
    pub failed_sequence: Option<OperationSequence>,
}

/// Recovery export failure that preserves the in-memory board.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RecoveryError {
    /// Recovery directory is unsafe or invalid.
    #[error("invalid recovery directory: {0}")]
    InvalidDirectory(String),
    /// JSON encoding failed.
    #[error("recovery serialization failed: {0}")]
    Serialization(String),
    /// Atomic filesystem operation failed.
    #[error("recovery I/O failed: {0}")]
    Io(String),
}

/// Writes a versioned recovery document to a user-only location.
pub trait RecoveryExporter {
    /// Atomically export one exact in-memory snapshot.
    ///
    /// # Errors
    ///
    /// Returns a typed error without modifying the source state.
    fn export(
        &mut self,
        request_id: RequestId,
        document: &RecoveryDocument,
    ) -> Result<PathBuf, RecoveryError>;
}
