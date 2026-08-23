//! Runtime ownership and schema-exclusion facade.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{InstanceId, SessionId, Timestamp};

/// Descriptive metadata for one process holding a session lease.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceInfo {
    /// Running process identity.
    pub instance_id: InstanceId,
    /// Session held by this process.
    pub session_id: SessionId,
    /// Operating-system process identifier.
    pub pid: u32,
    /// Proqi application version.
    pub version: String,
    /// Local storage protocol used by this process.
    pub storage_protocol: u32,
    /// Directory from which the process was launched.
    pub launch_directory: String,
    /// Process start time.
    pub started_at: Timestamp,
}

/// Verified runtime ownership and stale-crash recovery observed in one scan.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeScan {
    /// Sessions whose authoritative lock is currently held.
    pub active: Vec<InstanceInfo>,
    /// Sessions whose stale metadata was removed after finding no live lock.
    pub recovered: Vec<SessionId>,
}

/// Runtime coordination failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RuntimeError {
    /// Another process owns the requested session.
    #[error("session is already active: {session_id}")]
    SessionBusy {
        /// Contended session.
        session_id: SessionId,
        /// Best-effort holder metadata, never authoritative.
        holder: Option<InstanceInfo>,
    },
    /// Schema lease conflicts with another running version.
    #[error("schema is in use by another Proqi process")]
    SchemaBusy,
    /// Runtime filesystem operation failed.
    #[error("runtime coordination I/O failed: {0}")]
    Io(String),
    /// Descriptive metadata was malformed and could not be trusted.
    #[error("runtime metadata is malformed: {0}")]
    MalformedMetadata(String),
    /// Runtime paths or launch context are invalid.
    #[error("runtime coordination input is invalid: {0}")]
    Invalid(String),
}

/// Marker implemented by RAII leases.
pub trait Lease {}

/// Runtime ownership operations.
pub trait RuntimeCoordinator {
    /// Concrete session lease.
    type SessionLease: Lease;
    /// Concrete shared schema lease.
    type SharedSchemaLease: Lease;
    /// Concrete exclusive schema lease.
    type ExclusiveSchemaLease: Lease;

    /// Acquire authoritative exclusive ownership of one session.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::SessionBusy`] or a typed filesystem failure.
    fn acquire_session(&self, session_id: SessionId) -> Result<Self::SessionLease, RuntimeError>;

    /// Acquire a shared schema lease for a process using the current schema.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::SchemaBusy`] when migration owns the schema.
    fn acquire_schema_shared(&self) -> Result<Self::SharedSchemaLease, RuntimeError>;

    /// Attempt the exclusive schema lease required by migration.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::SchemaBusy`] while any process holds a shared lease.
    fn acquire_schema_exclusive(&self) -> Result<Self::ExclusiveSchemaLease, RuntimeError>;

    /// Return verified active instance metadata and clean stale entries.
    ///
    /// # Errors
    ///
    /// Returns a typed filesystem or metadata failure.
    fn scan_runtime(&self) -> Result<RuntimeScan, RuntimeError>;

    /// Return only verified active instances from one complete scan.
    ///
    /// # Errors
    ///
    /// Returns a typed filesystem or metadata failure.
    fn active_instances(&self) -> Result<Vec<InstanceInfo>, RuntimeError> {
        Ok(self.scan_runtime()?.active)
    }
}
