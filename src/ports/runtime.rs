//! Runtime ownership and schema-exclusion facade.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    InstallationIdentity, InstanceId, RequestId, SessionId, StableVersion, TextPosition, ThoughtId,
    Timestamp,
};

/// Content-free interaction checkpoint carried only across input recovery replacement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputRecoveryUiState {
    /// Exact non-modal interaction owner.
    pub mode: InputRecoveryMode,
    /// Focused thought when one exists.
    pub focused_thought: Option<ThoughtId>,
    /// Current logical insertion position.
    pub insertion_index: usize,
    /// Whether the insertion row owns Board focus.
    pub insertion_focused: bool,
    /// Whether Compose is showing its active editor rather than its passive prompt.
    pub compose_editor_visible: bool,
    /// Active editor geometry, without editor content.
    pub editor: Option<InputRecoveryEditorState>,
    /// Exact Board thought selection.
    pub selection: InputRecoverySelection,
    /// Expanded presentation folds by thought and annotation index.
    pub expanded_folds: Vec<(ThoughtId, usize)>,
    /// Projection-stable Board scroll position.
    pub board_viewport: InputRecoveryBoardViewport,
}

/// Supported non-modal interaction owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum InputRecoveryMode {
    /// Whole-thought Board interaction.
    Board,
    /// Empty transient insertion editor.
    Compose,
    /// Existing thought editor.
    Edit {
        /// Thought owning the editor.
        thought_id: ThoughtId,
    },
}

/// Content-free editor position retained across replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputRecoveryEditorState {
    /// Logical cursor head.
    pub cursor: TextPosition,
    /// Optional fixed directional selection endpoint.
    pub selection_anchor: Option<TextPosition>,
    /// Whether a soft-wrap boundary belongs to the previous row.
    pub previous_row_affinity: bool,
    /// First visible wrapped editor row.
    pub scroll_row: usize,
}

/// Exact content-free Board selection state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputRecoverySelection {
    /// Selected thoughts.
    pub selected: Vec<ThoughtId>,
    /// Optional contiguous range endpoints.
    pub range: Option<(ThoughtId, ThoughtId)>,
    /// Whether the keyboard range latch is active.
    pub latched: bool,
}

/// Board viewport ownership and semantic anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputRecoveryBoardViewport {
    /// Whether focus movement owns subsequent scrolling.
    pub follows_focus: bool,
    /// Current projection-stable anchor.
    pub anchor: InputRecoveryScrollAnchor,
}

/// Content-free Board scroll anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum InputRecoveryScrollAnchor {
    /// Beginning of the Board.
    Start,
    /// Gap preceding a thought.
    GapBefore {
        /// Thought following the gap.
        thought_id: ThoughtId,
        /// Row within the gap.
        row: usize,
    },
    /// Canonical or projected row within thought content.
    Content {
        /// Thought owning the content row.
        thought_id: ThoughtId,
        /// Nearest canonical UTF-8 byte boundary.
        canonical_byte: usize,
        /// Annotation owning a projected-only row.
        annotation_index: Option<usize>,
        /// Row within that annotation projection.
        projection_row: Option<usize>,
    },
    /// Tall-thought overflow marker.
    Overflow {
        /// Tall thought owning the overflow marker.
        thought_id: ThoughtId,
    },
    /// Compose editor content row.
    Compose {
        /// Canonical UTF-8 byte at the first visible Compose row.
        byte: usize,
    },
    /// Gap before the insertion row.
    InsertGap,
    /// Insertion row.
    Insert,
}

/// Process-local proof that this runtime was created by one accepted Unix replacement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateReplacementContext {
    /// Update attempt that authorized replacement.
    pub operation_id: RequestId,
    /// Exact old process image replaced in place.
    pub previous_instance_id: InstanceId,
    /// Target executable version verified immediately before `exec`.
    pub target_version: StableVersion,
}

/// Update-coordination capability advertised by a live process.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateInstanceContext {
    /// Stable installation identity shared across executable versions.
    pub installation_identity: InstallationIdentity,
    /// Ephemeral update-control protocol supported by this process.
    pub protocol: u32,
    /// Present only when this process image was entered through an accepted update replacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<Box<UpdateReplacementContext>>,
}

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
    /// Supported owner-control protocol, absent for older or unsupported processes.
    #[serde(default)]
    pub control_protocol: Option<u32>,
    /// User-only local control endpoint, absent when unavailable.
    #[serde(default)]
    pub control_endpoint: Option<String>,
    /// Optional all-session update coordination capability.
    #[serde(default)]
    pub update: Option<UpdateInstanceContext>,
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

/// Bounded descriptive metadata for the installation-wide screenshot owner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureOwnerInfo {
    /// Running process identity.
    pub instance_id: InstanceId,
    /// Session receiving new screenshots.
    pub session_id: SessionId,
    /// Operating-system process identifier.
    pub pid: u32,
    /// Proqi application version.
    pub version: String,
    /// Supported screenshot takeover protocol.
    pub capture_protocol: u32,
    /// Verified owner-control protocol.
    pub control_protocol: u32,
    /// User-only local control endpoint.
    pub control_endpoint: String,
    /// Process start time used to reject stale PID reuse.
    pub started_at: Timestamp,
}

/// Installation-wide screenshot ownership failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CaptureLockError {
    /// Another live compatible or incompatible process owns screenshot delivery.
    #[error("screenshot inbox is owned by another Proqi process")]
    Busy {
        /// Best-effort bounded owner metadata, never authoritative by itself.
        owner: Option<Box<CaptureOwnerInfo>>,
    },
    /// The session owner has no verified control endpoint for safe takeover.
    #[error("screenshot inbox requires verified owner control")]
    ControlUnavailable,
    /// Runtime filesystem operation failed.
    #[error("screenshot ownership I/O failed: {0}")]
    Io(String),
    /// Descriptive metadata was malformed.
    #[error("screenshot ownership metadata is malformed")]
    MalformedMetadata,
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
        holder: Option<Box<InstanceInfo>>,
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

/// Authoritative screenshot capture lease released on drop or process exit.
pub trait CaptureLease: Lease {
    /// Bounded owner metadata corresponding to the held lock.
    fn owner(&self) -> &CaptureOwnerInfo;
}

/// Separate installation-wide screenshot exclusion capability.
pub trait CaptureCoordinator {
    /// Concrete authoritative screenshot lease.
    type CaptureLease: CaptureLease;

    /// Attempt screenshot ownership for a verified interactive session owner.
    ///
    /// # Errors
    ///
    /// Returns busy owner metadata or a typed runtime failure. It never waits or force-unlocks.
    fn acquire_capture(
        &self,
        instance: &InstanceInfo,
    ) -> Result<Self::CaptureLease, CaptureLockError>;
}

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
